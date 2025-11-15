use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use std::collections::HashSet;
use syn::fold::Fold;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    parse_quote_spanned, BinOp, Block, Expr, ExprAssign, ExprForLoop, ExprLoop, ExprWhile, FnArg,
    Ident, ItemFn, LitStr, Pat, Result, Stmt, Token,
};

#[proc_macro_attribute]
pub fn recorded(attr: TokenStream, item: TokenStream) -> TokenStream {
    let recorder_ident = match parse_recorder_ident(attr) {
        Ok(ident) => ident,
        Err(err) => return TokenStream::from(err.to_compile_error()),
    };

    let mut function = syn::parse_macro_input!(item as ItemFn);
    if !function_has_recorder_arg(&function, &recorder_ident) {
        let error = syn::Error::new(
            function.sig.span(),
            format!(
                "`#[recorded]` requires a `{}` argument in the function signature",
                recorder_ident
            ),
        )
        .to_compile_error();
        return TokenStream::from(quote! {
            #function
            #error
        });
    }

    let mut folder = RecorderFolder {
        recorder_ident: recorder_ident.clone(),
        closure_depth: 0,
        suppress_instrumentation: 0,
        bool_scopes: Vec::new(),
        block_depth: 0,
    };
    let block = folder.fold_block(*function.block);
    function.block = Box::new(block);

    TokenStream::from(quote! {
        #function
    })
}

struct EnumeratedArrayLoop {
    array_ident: Ident,
    index_ident: Ident,
    value_ident: Ident,
}

struct RangeLoop {
    index_ident: Ident,
}

struct RecorderFolder {
    recorder_ident: Ident,
    closure_depth: usize,
    suppress_instrumentation: usize,
    bool_scopes: Vec<HashSet<String>>,
    block_depth: usize,
}

impl RecorderFolder {
    fn wrap_with_step(&self, expr: Expr, span: Span) -> Expr {
        let line = span.start().line as u32;
        let recorder = &self.recorder_ident;
        parse_quote_spanned! {span=>
            {
                #recorder.push_step(#line);
                #expr
            }
        }
    }

    fn loop_label(&self, span: Span) -> (u32, LitStr) {
        let line = span.start().line as u32;
        let label = LitStr::new(&format!("loop@{}", line), Span::call_site());
        (line, label)
    }

    fn instrument_for_loop(&self, mut expr_for: ExprForLoop, span: Span) -> Expr {
        let (line, label) = self.loop_label(span);
        let recorder = &self.recorder_ident;
        let iter_ident = format_ident!(
            "__recorded_loop_iter_{}_{}",
            span.start().line,
            span.start().column
        );
        let current_ident = format_ident!(
            "__recorded_loop_iter_value_{}_{}",
            span.start().line,
            span.start().column
        );
        let body = expr_for.body;
        expr_for.body = parse_quote_spanned! {span=>
            {
                #recorder.push_step(#line);
                let #current_ident = #iter_ident;
                #iter_ident += 1;
                #recorder.push_number_step(#line, #label, &#current_ident);
                #body
            }
        };

        parse_quote_spanned! {span=>
            {
                let mut #iter_ident: usize = 0;
                #expr_for
            }
        }
    }

    fn instrument_while_loop(&self, mut expr_while: ExprWhile, span: Span) -> Expr {
        let (line, label) = self.loop_label(span);
        let recorder = &self.recorder_ident;
        let iter_ident = format_ident!(
            "__recorded_loop_iter_{}_{}",
            span.start().line,
            span.start().column
        );
        let current_ident = format_ident!(
            "__recorded_loop_iter_value_{}_{}",
            span.start().line,
            span.start().column
        );
        let body = expr_while.body;
        expr_while.body = parse_quote_spanned! {span=>
            {
                #recorder.push_step(#line);
                let #current_ident = #iter_ident;
                #iter_ident += 1;
                #recorder.push_number_step(#line, #label, &#current_ident);
                #body
            }
        };

        parse_quote_spanned! {span=>
            {
                let mut #iter_ident: usize = 0;
                #expr_while
            }
        }
    }

    fn instrument_loop(&self, mut expr_loop: ExprLoop, span: Span) -> Expr {
        let (line, label) = self.loop_label(span);
        let recorder = &self.recorder_ident;
        let iter_ident = format_ident!(
            "__recorded_loop_iter_{}_{}",
            span.start().line,
            span.start().column
        );
        let current_ident = format_ident!(
            "__recorded_loop_iter_value_{}_{}",
            span.start().line,
            span.start().column
        );
        let body = expr_loop.body;
        expr_loop.body = parse_quote_spanned! {span=>
            {
                #recorder.push_step(#line);
                let #current_ident = #iter_ident;
                #iter_ident += 1;
                #recorder.push_number_step(#line, #label, &#current_ident);
                #body
            }
        };

        parse_quote_spanned! {span=>
            {
                let mut #iter_ident: usize = 0;
                #expr_loop
            }
        }
    }

    fn fold_for_loop_expr(&mut self, expr_for: ExprForLoop) -> Expr {
        let enumerated_info = extract_enumerated_array_loop(&expr_for);
        let range_info = if enumerated_info.is_some() {
            None
        } else {
            detect_range_loop(&expr_for)
        };

        let span = expr_for.span();
        let line = span.start().line as u32;
        let ExprForLoop {
            attrs,
            label,
            for_token,
            pat,
            in_token,
            expr,
            body,
        } = expr_for;

        let attrs = self.fold_attributes(attrs);
        let label = label.map(|label| self.fold_label(label));
        let pat = Box::new(self.fold_pat(*pat));

        self.suppress_instrumentation += 1;
        let expr = Box::new(self.fold_expr(*expr));
        self.suppress_instrumentation -= 1;

        let mut body = if enumerated_info.is_some() {
            self.fold_block_suppressed(body)
        } else {
            self.fold_block(body)
        };
        let recorder = &self.recorder_ident;
        if enumerated_info.is_none() {
            let step_stmt: Stmt = parse_quote_spanned! {span=>
                {
                    #recorder.push_step(#line);
                }
            };
            body.stmts.insert(0, step_stmt);

            if let Some(RangeLoop { index_ident }) = range_info {
                let index_name = LitStr::new(&index_ident.to_string(), Span::call_site());
                body.stmts.insert(1, parse_quote_spanned! {span=>
                    {
                        #recorder.push_number_step(#line, #index_name, &#index_ident);
                    }
                });
            }
        }

        if let Some(info) = enumerated_info {
            let EnumeratedArrayLoop {
                array_ident,
                index_ident,
                value_ident,
            } = info;
            let array_name = LitStr::new(&array_ident.to_string(), Span::call_site());
            body.stmts.insert(0, parse_quote_spanned! {span=>
                {
                    #recorder.push_step(#line);
                }
            });
            body.stmts.push(parse_quote_spanned! {span=>
                {
                    #recorder.push_array_step(#line, #array_name, #index_ident, &*#value_ident);
                }
            });
            return Expr::ForLoop(ExprForLoop {
                attrs,
                label,
                for_token,
                pat,
                in_token,
                expr,
                body,
            });
        }

        let expr_for = ExprForLoop {
            attrs,
            label,
            for_token,
            pat,
            in_token,
            expr,
            body,
        };
        self.instrument_for_loop(expr_for, span)
    }

    fn fold_while_loop_expr(&mut self, expr_while: ExprWhile) -> Expr {
        let span = expr_while.span();
        let ExprWhile {
            attrs,
            label,
            while_token,
            cond,
            body,
        } = expr_while;

        let attrs = self.fold_attributes(attrs);
        let label = label.map(|label| self.fold_label(label));

        self.suppress_instrumentation += 1;
        let cond = Box::new(self.fold_expr(*cond));
        self.suppress_instrumentation -= 1;

        let body = self.fold_block(body);
        let expr_while = ExprWhile {
            attrs,
            label,
            while_token,
            cond,
            body,
        };
        self.instrument_while_loop(expr_while, span)
    }

    fn fold_loop_expr(&mut self, expr_loop: ExprLoop) -> Expr {
        let span = expr_loop.span();
        let ExprLoop {
            attrs,
            label,
            loop_token,
            body,
        } = expr_loop;

        let attrs = self.fold_attributes(attrs);
        let label = label.map(|label| self.fold_label(label));
        let body = self.fold_block(body);
        let expr_loop = ExprLoop {
            attrs,
            label,
            loop_token,
            body,
        };
        self.instrument_loop(expr_loop, span)
    }

    fn instrument_stmt(&mut self, stmt: Stmt, in_root_block: bool, out: &mut Vec<Stmt>) {
        match stmt {
            Stmt::Local(local) => {
                let span = local.span();
                let line = span.start().line as u32;
                let ident = ident_from_pat(&local.pat);
                let has_init = local.init.is_some();
                out.push(Stmt::Local(local));

                if has_init {
                    if let Some(ident) = ident.clone() {
                        let recorder = &self.recorder_ident;
                        let name = LitStr::new(&ident.to_string(), Span::call_site());
                        let stmt = if in_root_block {
                            parse_quote_spanned! {ident.span()=>
                                #recorder.set_initial(#name, &#ident);
                            }
                        } else {
                            self.record_variable_stmt(&ident, line)
                        };
                        out.push(stmt);
                    }
                }
            }
            Stmt::Item(item) => out.push(Stmt::Item(item)),
            Stmt::Expr(expr, semi) => {
                let expr = self.instrument_assignment_expr(expr);
                out.push(Stmt::Expr(expr, semi));
            }
            Stmt::Macro(mac) => out.push(Stmt::Macro(mac)),
        }
    }

    fn instrument_assignment_expr(&self, expr: Expr) -> Expr {
        match expr {
            Expr::Assign(assign) => {
                if let Some(new_expr) = self.wrap_assign(&assign) {
                    new_expr
                } else {
                    Expr::Assign(assign)
                }
            }
            Expr::Binary(binary) => {
                if let Some(new_expr) = self.wrap_binary_assign(&binary) {
                    new_expr
                } else {
                    Expr::Binary(binary)
                }
            }
            other => other,
        }
    }

    fn wrap_assign(&self, assign: &ExprAssign) -> Option<Expr> {
        let target = assignment_target(assign.left.as_ref())?;
        let span = assign.span();
        let line = span.start().line as u32;
        let recorder = &self.recorder_ident;

        match target {
            AssignmentTarget::Variable(ident) => {
                let record_stmt = self.record_variable_stmt(&ident, line);
                let assign_expr = assign.clone();
                Some(parse_quote_spanned! {span=>
                    {
                        #assign_expr;
                        #record_stmt
                    }
                })
            }
            AssignmentTarget::Array { ident, index } => {
                let array_name = LitStr::new(&ident.to_string(), Span::call_site());
                let value = (*assign.right).clone();
                let index_tmp = format_ident!(
                    "__recorded_index_{}_{}",
                    span.start().line,
                    span.start().column
                );
                Some(parse_quote_spanned! {span=>
                    {
                        let #index_tmp = #index;
                        #ident[#index_tmp] = #value;
                        #recorder.push_array_step(#line, #array_name, #index_tmp, &#ident[#index_tmp]);
                    }
                })
            }
        }
    }

    fn wrap_binary_assign(&self, binary: &syn::ExprBinary) -> Option<Expr> {
        if !is_assign_op(&binary.op) {
            return None;
        }

        let target = assignment_target(binary.left.as_ref())?;
        let span = binary.span();
        let line = span.start().line as u32;
        let recorder = &self.recorder_ident;

        match target {
            AssignmentTarget::Variable(ident) => {
                let record_stmt = self.record_variable_stmt(&ident, line);
                let binary_expr = binary.clone();
                Some(parse_quote_spanned! {span=>
                    {
                        #binary_expr;
                        #record_stmt
                    }
                })
            }
            AssignmentTarget::Array { ident, index } => {
                let array_name = LitStr::new(&ident.to_string(), Span::call_site());
                let value = (*binary.right).clone();
                let op = binary.op.clone();
                let index_tmp = format_ident!(
                    "__recorded_index_{}_{}",
                    span.start().line,
                    span.start().column
                );
                Some(parse_quote_spanned! {span=>
                    {
                        let #index_tmp = #index;
                        #ident[#index_tmp] #op #value;
                        #recorder.push_array_step(#line, #array_name, #index_tmp, &#ident[#index_tmp]);
                    }
                })
            }
        }
    }

    fn record_variable_stmt(&self, ident: &Ident, line: u32) -> Stmt {
        let recorder = &self.recorder_ident;
        let name = LitStr::new(&ident.to_string(), Span::call_site());
        if self.is_bool_ident(ident) {
            parse_quote_spanned! {ident.span()=>
                #recorder.push_bool_step(#line, #name, &#ident);
            }
        } else {
            parse_quote_spanned! {ident.span()=>
                #recorder.push_number_step(#line, #name, &#ident);
            }
        }
    }

    fn is_bool_ident(&self, ident: &Ident) -> bool {
        let name = ident.to_string();
        self.bool_scopes
            .iter()
            .rev()
            .any(|scope| scope.contains(&name))
    }
}

impl Fold for RecorderFolder {
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        if self.suppress_instrumentation > 0 {
            return syn::fold::fold_expr(self, expr);
        }

        if matches!(expr, Expr::Closure(_)) {
            self.closure_depth += 1;
            let expr = syn::fold::fold_expr(self, expr);
            self.closure_depth -= 1;
            return expr;
        }

        if self.closure_depth > 0 {
            return syn::fold::fold_expr(self, expr);
        }

        match expr {
            Expr::ForLoop(expr_for) => return self.fold_for_loop_expr(expr_for),
            Expr::While(expr_while) => return self.fold_while_loop_expr(expr_while),
            Expr::Loop(expr_loop) => return self.fold_loop_expr(expr_loop),
            _ => {}
        }

        let span = expr.span();
        let expr = syn::fold::fold_expr(self, expr);

        match expr {
            Expr::If(expr_if) => self.wrap_with_step(Expr::If(expr_if), span),
            Expr::Match(expr_match) => self.wrap_with_step(Expr::Match(expr_match), span),
            Expr::Call(expr_call) => self.wrap_with_step(Expr::Call(expr_call), span),
            Expr::MethodCall(expr_method) => {
                self.wrap_with_step(Expr::MethodCall(expr_method), span)
            }
            other => other,
        }
    }

    fn fold_block(&mut self, block: Block) -> Block {
        if self.suppress_instrumentation > 0 {
            return syn::fold::fold_block(self, block);
        }

        self.block_depth += 1;
        let depth = self.block_depth;
        let bools = if self.suppress_instrumentation == 0 {
            collect_bool_idents(&block)
        } else {
            HashSet::new()
        };
        self.bool_scopes.push(bools);
        let block = syn::fold::fold_block(self, block);

        let mut new_stmts = Vec::with_capacity(block.stmts.len());
        for stmt in block.stmts {
            self.instrument_stmt(stmt, depth == 1 && self.suppress_instrumentation == 0, &mut new_stmts);
        }

        self.bool_scopes.pop();
        self.block_depth -= 1;

        Block {
            brace_token: block.brace_token,
            stmts: new_stmts,
        }
    }

}

fn parse_recorder_ident(attr: TokenStream) -> Result<Ident> {
    if attr.is_empty() {
        return Ok(Ident::new("recorder", Span::call_site()));
    }

    syn::parse::<RecordedArgs>(attr).map(|args| args.recorder)
}

struct RecordedArgs {
    recorder: Ident,
}

impl Parse for RecordedArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        if key != "recorder" {
            return Err(syn::Error::new(key.span(), "expected `recorder = <ident>`"));
        }

        input.parse::<Token![=]>()?;
        let recorder: Ident = input.parse()?;

        if !input.is_empty() {
            let span = input.span();
            return Err(syn::Error::new(
                span,
                "unexpected tokens after recorder argument",
            ));
        }

        Ok(Self { recorder })
    }
}

fn function_has_recorder_arg(function: &ItemFn, recorder_ident: &Ident) -> bool {
    function.sig.inputs.iter().any(|arg| match arg {
        FnArg::Typed(pat_type) => matches_recorder(pat_type.pat.as_ref(), recorder_ident),
        FnArg::Receiver(_) => false,
    })
}

fn matches_recorder(pat: &Pat, recorder_ident: &Ident) -> bool {
    match pat {
        Pat::Ident(pat_ident) => pat_ident.ident == *recorder_ident,
        Pat::Type(pat_type) => matches_recorder(pat_type.pat.as_ref(), recorder_ident),
        Pat::Reference(pat_ref) => matches_recorder(pat_ref.pat.as_ref(), recorder_ident),
        Pat::Paren(pat_paren) => matches_recorder(pat_paren.pat.as_ref(), recorder_ident),
        _ => false,
    }
}

fn ident_from_pat(pat: &Pat) -> Option<Ident> {
    match pat {
        Pat::Ident(pat_ident) => {
            let ident = pat_ident.ident.clone();
            if ident == "_" {
                return None;
            }
            Some(ident)
        }
        Pat::Type(pat_type) => ident_from_pat(pat_type.pat.as_ref()),
        Pat::Reference(pat_ref) => ident_from_pat(pat_ref.pat.as_ref()),
        Pat::Paren(pat_paren) => ident_from_pat(pat_paren.pat.as_ref()),
        _ => None,
    }
}

fn tuple_index_and_value_idents(pat: &Pat) -> Option<(Ident, Ident)> {
    if let Pat::Tuple(tuple) = pat
        && tuple.elems.len() == 2
    {
        let index_ident = match &tuple.elems[0] {
            Pat::Ident(ident) => ident.ident.clone(),
            _ => return None,
        };
        let value_ident = match &tuple.elems[1] {
            Pat::Ident(ident) => ident.ident.clone(),
            _ => return None,
        };
        return Some((index_ident, value_ident));
    }
    None
}

fn extract_enumerated_array_loop(expr_for: &ExprForLoop) -> Option<EnumeratedArrayLoop> {
    let (index_ident, value_ident) = tuple_index_and_value_idents(expr_for.pat.as_ref())?;

    fn base_array_ident(expr: &Expr) -> Option<Ident> {
        if let Expr::Path(path) = expr
            && path.path.segments.len() == 1
        {
            return Some(path.path.segments[0].ident.clone());
        }
        None
    }

    fn is_method(call: &syn::ExprMethodCall, name: &str) -> bool {
        call.method == name && call.args.is_empty()
    }

    let array_ident = match expr_for.expr.as_ref() {
        Expr::MethodCall(enumerate_call) if is_method(enumerate_call, "enumerate") => {
            if let Expr::MethodCall(iter_call) = enumerate_call.receiver.as_ref() {
                if is_method(iter_call, "iter_mut") {
                    base_array_ident(iter_call.receiver.as_ref())
                } else {
                    None
                }
            } else {
                None
            }
        }
        _ => None,
    }?;

    Some(EnumeratedArrayLoop {
        array_ident,
        index_ident,
        value_ident,
    })
}

fn detect_range_loop(expr_for: &ExprForLoop) -> Option<RangeLoop> {
    let index_ident = ident_from_pat(expr_for.pat.as_ref())?;
    match expr_for.expr.as_ref() {
        Expr::Range(_) => Some(RangeLoop { index_ident }),
        _ => None,
    }
}

enum AssignmentTarget {
    Variable(Ident),
    Array { ident: Ident, index: Expr },
}

fn assignment_target(expr: &Expr) -> Option<AssignmentTarget> {
    match expr {
        Expr::Path(expr_path) => ident_from_expr_path(expr_path).map(AssignmentTarget::Variable),
        Expr::Index(expr_index) => {
            if let Some(ident) = ident_from_expr(expr_index.expr.as_ref()) {
                let index = (*expr_index.index).clone();
                Some(AssignmentTarget::Array { ident, index })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn ident_from_expr(expr: &Expr) -> Option<Ident> {
    match expr {
        Expr::Path(expr_path) => ident_from_expr_path(expr_path),
        _ => None,
    }
}

fn ident_from_expr_path(expr_path: &syn::ExprPath) -> Option<Ident> {
    if expr_path.qself.is_none() && expr_path.path.segments.len() == 1 {
        Some(expr_path.path.segments[0].ident.clone())
    } else {
        None
    }
}

fn collect_bool_idents(block: &Block) -> HashSet<String> {
    let mut collector = BoolCollector::default();
    collector.visit_block(block);
    collector.idents
}

fn is_assign_op(op: &BinOp) -> bool {
    matches!(
        op,
        BinOp::AddAssign(_)
            | BinOp::SubAssign(_)
            | BinOp::MulAssign(_)
            | BinOp::DivAssign(_)
            | BinOp::RemAssign(_)
            | BinOp::BitXorAssign(_)
            | BinOp::BitAndAssign(_)
            | BinOp::BitOrAssign(_)
            | BinOp::ShlAssign(_)
            | BinOp::ShrAssign(_)
    )
}

#[derive(Default)]
struct BoolCollector {
    idents: HashSet<String>,
}

impl<'ast> Visit<'ast> for BoolCollector {
    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        if let Some(ident) = ident_from_expr(&node.cond) {
            self.idents.insert(ident.to_string());
        }
        syn::visit::visit_expr_if(self, node);
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        if let Some(ident) = ident_from_expr(&node.cond) {
            self.idents.insert(ident.to_string());
        }
        syn::visit::visit_expr_while(self, node);
    }
}

impl RecorderFolder {
    fn fold_block_suppressed(&mut self, block: Block) -> Block {
        self.suppress_instrumentation += 1;
        let block = syn::fold::fold_block(self, block);
        self.suppress_instrumentation -= 1;
        block
    }
}

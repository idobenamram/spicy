use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::fold::Fold;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Block, Expr, ExprForLoop, ExprLoop, ExprWhile, FnArg, Ident, ItemFn, LitStr, Pat, Result, Stmt,
    Token, parse_quote_spanned,
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
        depth: 0,
        closure_depth: 0,
    };
    let block = folder.fold_block(*function.block);
    function.block = Box::new(block);

    TokenStream::from(quote! {
        #function
    })
}

struct RecorderFolder {
    recorder_ident: Ident,
    depth: usize,
    closure_depth: usize,
}

impl RecorderFolder {
    fn wrap_with_step(&self, expr: Expr, span: Span) -> Expr {
        let line = span.start().line as u32;
        let recorder = &self.recorder_ident;
        parse_quote_spanned! {span=>
            {
                #recorder.push_step(#line - 1);
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
                let #current_ident = #iter_ident;
                #iter_ident += 1;
                #recorder.push_number_step(#line - 1, #label, &#current_ident);
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
                let #current_ident = #iter_ident;
                #iter_ident += 1;
                #recorder.push_number_step(#line - 1, #label, &#current_ident);
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
                let #current_ident = #iter_ident;
                #iter_ident += 1;
                #recorder.push_number_step(#line - 1, #label, &#current_ident);
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
}

impl Fold for RecorderFolder {
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        if matches!(expr, Expr::Closure(_)) {
            self.closure_depth += 1;
            let expr = syn::fold::fold_expr(self, expr);
            self.closure_depth -= 1;
            return expr;
        }

        let span = expr.span();
        let expr = syn::fold::fold_expr(self, expr);

        if self.closure_depth > 0 {
            return expr;
        }

        match expr {
            Expr::If(expr_if) => self.wrap_with_step(Expr::If(expr_if), span),
            Expr::While(expr_while) => {
                let instrumented = self.instrument_while_loop(expr_while, span);
                self.wrap_with_step(instrumented, span)
            }
            Expr::ForLoop(expr_for) => {
                let instrumented = self.instrument_for_loop(expr_for, span);
                self.wrap_with_step(instrumented, span)
            }
            Expr::Loop(expr_loop) => {
                let instrumented = self.instrument_loop(expr_loop, span);
                self.wrap_with_step(instrumented, span)
            }
            Expr::Match(expr_match) => self.wrap_with_step(Expr::Match(expr_match), span),
            Expr::Call(expr_call) => self.wrap_with_step(Expr::Call(expr_call), span),
            Expr::MethodCall(expr_method) => {
                self.wrap_with_step(Expr::MethodCall(expr_method), span)
            }
            other => other,
        }
    }

    fn fold_block(&mut self, block: Block) -> Block {
        self.depth += 1;
        let depth = self.depth;
        let block = syn::fold::fold_block(self, block);
        self.depth -= 1;

        if depth != 1 {
            return block;
        }

        let mut new_stmts = Vec::with_capacity(block.stmts.len());

        for stmt in block.stmts {
            match stmt {
                Stmt::Local(local) => {
                    let ident = ident_from_pat(&local.pat);
                    let has_init = local.init.is_some();
                    new_stmts.push(Stmt::Local(local));

                    if has_init {
                        if let Some(ident) = ident {
                            let recorder = &self.recorder_ident;
                            let name = ident.to_string();
                            let record_stmt: Stmt = syn::parse_quote! {
                                #recorder.set_initial(#name, &#ident);
                            };
                            new_stmts.push(record_stmt);
                        }
                    }
                }
                other => new_stmts.push(other),
            }
        }

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
        if key.to_string() != "recorder" {
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
            if ident.to_string() == "_" {
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

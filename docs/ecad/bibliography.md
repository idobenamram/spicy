# Engine Bibliography

> Companion to `engine.md` (originally v2) · 2026-09-24
> Keys in **[brackets]** are the ones cited in the v2 notes.

**How this was checked.** Every entry was checked against a primary record (publisher page, DOI/Crossref, OSTI, arXiv, or the PDF itself).

- Unmarked entries: title, authors, venue and year were confirmed.
- **(partial)**: the work exists, but a detail (volume, pages, or a specific claim) wasn't confirmed.
- **(abstract only)**: content known from the abstract, not the full text.
- Corrections to v1 or to earlier leads are called out where they happened.

---

## Start here

If you only read a few things, read these, in this order.

| Key | Why |
|---|---|
| [N&P07] | The corner theorem (Thm 5.1) and the rank-1 structure behind it; also the counterexamples. The core of A5. |
| [R&K15] | How to implement affine arithmetic well. Its `kv` library (MIT) is the best implementation reference. |
| [Graeb07] | Range vs statistical parameters, worst-case distance, design centering. The core of A7–A8. |
| [ECSS11] | Practical worst-case analysis handbook: biased vs random effects and four analysis methods, written for engineers. Free PDF. |
| [Xyce16] | How a real simulator implements sensitivities (direct + adjoint). Read before building Part D. |
| [FS00] | The outer + inner bracket idea applied to circuits. |

---

## 1. Affine arithmetic and its relatives

- **[CS93]** Comba & Stolfi, "Affine arithmetic and its applications to computer graphics," Proc. VI SIBGRAPI, pp. 9–18, 1993. The origin of AA.
- **[Stolfi97]** de Figueiredo & Stolfi, *Self-Validated Numerical Methods and Applications*, IMPA monograph, 1997 (ic.unicamp.br/~stolfi).
  - The actual source for how to pick the straight line (Chebyshev vs min-range), §3.7–3.16.
  - Chebyshev gives the smallest error term, but its range can overshoot (1/x on [1, 2] gives [0.414, 1]). Min-range never overshoots the true range but keeps less correlation. Lesson 1 shows both.
- de Figueiredo & Stolfi, "Affine arithmetic: concepts and applications," *Numer. Algorithms* 37:147–158, 2004. doi:10.1023/B:NUMA.0000049462.70970.b6. A survey.
- **[R&K15]** Rump & Kashiwagi, "Implementation and improvements of affine arithmetic," *NOLTA* 6(3):341–359, 2015. doi:10.1587/nolta.6.341.
  - Carry an interval next to each affine form. This fixes failures like 1/(C·C) with C = 2 ± 1.
  - Best-fit lines for about 30 elementary functions.
  - Warns that dropping small terms is unsafe.
- Messine, "Extentions of affine arithmetic…," *J. UCS* 8(11):992–1015, 2002. Fixed-length forms: input knobs plus 1–3 error terms (the "one error bucket" in our crate).
- Vu, Sam-Haroud & Faltings, *Annals Math. AI* 55(3–4):295–354, 2009. Coined "revised affine forms". Kolev's "linear interval enclosure" (*Reliable Computing* 7(1):17–28, 2001; *Numer. Algorithms* 37:213–224, 2004) is the same idea.
- Skalna & Hladík, *Numer. Algorithms* 76(4):1131–1152, 2017. Best-fit product of revised forms in linear time.
- Kiel, "YalAA," *Reliable Computing* 16:114–129, 2012. A survey of AA variants; its policy-based design maps well onto Rust traits.
- Ninin, Messine & Hansen, *4OR* 13(3):247–277, 2015. Turns affine forms into linear-programming relaxations inside branch-and-bound (IBEX's ibex-affine plugin).
- Makino & Berz, "Taylor models and other validated functional inclusion methods," *Int. J. Pure Appl. Math.* 4(4):379–456, 2003. The higher-order generalization.
- Neumaier, "Taylor forms—use and limits," *Reliable Computing* 9(1):43–79, 2003. Higher order only pays off with heavy cancellation.
- **[GGP09]** Ghorbal, Goubault & Putot, "The zonotope abstract domain Taylor1+," CAV 2009, LNCS 5643:627–633. Separates input knobs from internal error terms, and only compresses the latter. The policy we adopt.
- Ghorbal, Goubault & Putot, "A logical product approach to zonotope intersection," CAV 2010, LNCS 6174:212–226. Combining affine forms with constraints (AA + contractors).
- Polynomial zonotopes: Althoff, HSCC 2013, pp. 173–182; Kochdumper & Althoff, *IEEE TAC* 66(9):4043–4058, 2021.
- Zonotope order reduction (the same problem as merging error terms): Girard, HSCC 2005; Kopetzki et al., CDC 2017.
- **Inner bounds:**
  - Goubault & Putot, SAS 2007, LNCS 4634:137–152. Generalized AA gives guaranteed-reached inner ranges.
  - Goubault, Mullier, Putot & Kieffer, HSCC 2014.
  - Goubault & Putot, *IEEE L-CSS* 4(4):928–933, 2020. Inner ranges under adversarial parameters.
  - Goldsztejn & Jaulin, *Reliable Computing* 14:1–23, 2010.
  - Gardeñes et al., "Modal intervals," *Reliable Computing* 7(2):77–111, 2001.

## 2. Interval and affine methods applied to circuits

- **[FS00]** Femia & Spagnuolo, "True worst-case circuit tolerance analysis using genetic algorithms and affine arithmetic," *IEEE TCAS-I* 47(9):1285–1296, 2000. doi:10.1109/81.883323. AA outer bound plus a genetic-algorithm inner bound, so the true worst case is bracketed. (abstract only; no accuracy numbers found. Author order uncertain: Crossref lists Spagnuolo first, Semantic Scholar Femia first; check the PDF before citing.) Precursor: *TCAS-I* 46(12):1441–1456, 1999.
- Kolev, *Interval Methods for Circuit Analysis*, World Scientific, 1993. doi:10.1142/2039.
- Kolev, "Worst-case tolerance analysis of linear DC and AC electric circuits," *IEEE TCAS-I* 49(12):1693–1701, 2002. doi:10.1109/TCSI.2002.805700. Outer bound by a direct method, inner by iteration; exact once directions are proven.
- **[GOB08]** Grabowski, Olbrich & Barke, "Analog circuit simulation using range arithmetics," ASP-DAC 2008, pp. 762–767.
  - AA inside a SPICE-like solver.
  - Quadratic forms shrink the ranges by 7–40% at 1.7–9× the runtime.
  - Transient is called an "open issue". Checked against corners, not proofs.
- **[Sch18]** Scharf, dissertation, Leibniz Univ. Hannover, 2018 (repo.uni-hannover.de/handle/123456789/3695); Scharf, Olbrich & Barke, SIMUTOOLS 2015.
  - Without splitting, the AA solver converged only for tolerances of about 0.8–5.5% on diff-amp and BJT circuits.
  - Floating-point rounding is ignored, so the results aren't rigorous.
- Lemke, Hedrich & Barke, "Analog circuit sizing based on formal methods using affine arithmetic," ICCAD 2002, pp. 486–489. AA Newton + branch-and-bound for sizing. **The closest prior art to our engine.**
- **[Dre06]** Dreyer, PhD thesis, TU Kaiserslautern, 2005 (ISBN unconfirmed), and "Interval methods for analog circuits," ITWM Bericht 97, 2006. Rank-1 structure + Sherman–Morrison to prove monotonicity; AC in 2n-dimensional real form; µA741 example.
- **[Din15]** Ding, Trinchero, Manfredi, Stievano & Canavero, "How affine arithmetic helps beat uncertainties in electrical systems," *IEEE CAS Mag.* 15(4):70–79, 2015. Complex AA inside MNA; only a "rough" bound near resonance without splitting.
- **[TS00]** Tian & Shi, *IEEE TCAS-I* 47(8):1138–1145, 2000. Vertex analysis can miss the worst case when directions are guessed; uses interval "sensitivity bands" to prove them.
- Grimm, Heupke & Waldschmidt, DATE 2004, pp. 372–377, and *IEEE TCAD* 24(1):118–123, 2005. Block-level signal-flow models, not netlists.
- Ma & Rutenbar: ICCAD 2004, pp. 460–467; *IEEE TCAD* 26(9):1602–1613, 2007; and "Fast interval-valued statistical modeling of interconnect and effective capacitance," *IEEE TCAD* 25(4):710–724, **2006**.
- Fang, Rutenbar, Püschel & Chen, DAC 2003, pp. 496–501; Singhee, Fang, Ma & Rutenbar, DAC 2006, pp. 167–172. "Probabilistic AA": gives up containment on purpose, so **not guaranteed**.
- Vaccaro, Cañizares & Villacci, *IEEE TPWRS* 25(2):624–632, 2010. AA power flow, a nonlinear network cousin.

## 3. Circuit equations with uncertain parameters: A(p)·x = b(p)

MNA with tolerances is exactly this problem.

- **[N&P07]** Neumaier & Pownuk, "Linear systems with large uncertainties, with applications to truss structures," *Reliable Computing* 13(2):149–172, 2007. doi:10.1007/s11155-006-9026-1. Read in full.
  - Handles (K + B·D·A)·u = a + F·b with uncertain diagonal D. MNA fits: each conductance or controlled-source gain is one diagonal entry.
  - Truss, 101 parameters (Ex. 7.1): 0.6% overestimation at 1% uncertainty, 2.9% at 5%. Cost: about 20–25× one solve at 420 parameters (Ex. 7.2), growing to ~1,100–1,300× at 10,050 parameters (Ex. 7.3); rounding ignored.
  - Warns that interval monotonicity proofs "typically fail in high dimensions".
  - **Thm 5.1: extremes at the corners.** Ex. (39): interior maximum when a parameter appears twice. Ex. (41): the nominal gradient points to the wrong corner.
  - "k%" in the paper means total width k%, i.e. ±k/2%.
- **[Kol14a]** Kolev, "Parameterized solution…," *Appl. Math. Comput.* 246:229–246, 2014. The "p-solution" x(p) ∈ L·p + remainder: literally an affine form in the original knobs.
- **[Kol14b]** Kolev, *Reliable Computing* 20:1–24, 2014. Exact range when the vertex (corner) property holds.
- Kolev, *Reliable Computing* 8(6):493–501, 2002, and 10(3):227–239, 2004.
- Skalna, *Reliable Computing* 12:107–120, 2006; *Parametric Interval Algebraic Systems*, Springer SCI 766, 2018, doi:10.1007/978-3-319-75187-0.
- Skalna & Hladík, p-solution paper, *BIT* 57:1109–1136, 2017.
- **[SH18]** Skalna & Hladík, "Enhancing monotonicity checking…," TNC'18, Kalpa Publ. Comput. 8:70–83, 2018. On an 11-branch AC circuit, the inner/outer width ratio falls from 0.99–1.00 at ±5% to 0.28–0.54 at ±20%.
- **[PKK10]** Popova, Kolev & Krämer, "A solver for complex-valued parametric linear systems," *Serdica J. Computing* 4(1):123–132, 2010. AC circuit examples.
- Popova: chapter in *Scientific Computing, Validated Numerics, Interval Methods*, Kluwer 2001 (doi:10.1007/978-1-4757-6484-0_11); Popova & Krämer, *JCAM* 199(2):310–316, 2007 (inner and outer); Zimmer, Krämer & Popova, *Computing* 94(2–4):109–123, 2012; "Rank one interval enclosure…," *BIT* 59(2):503–521, 2019 (names circuits and trusses as rank-one cases); *Appl. Math. Comput.* 378:125205, 2020 (bounds on derived quantities like branch currents).
- Hladík, *IJAMCS* 22(3):561–574, 2012.
- Rump, "Verification methods: rigorous results using floating-point arithmetic," *Acta Numerica* 19:287–449, 2010.
- Rohn & Kreinovich, *SIMAX* 16:415–420, 1995: exact bounds are NP-hard in general. Rohn, *Reliable Computing* 11:129–135, 2005: midpoint preconditioning can overestimate by 100%.

## 4. Why corners work: bilinear network functions

- **[PPC65]** Parker, Peskin & Chirlian, "Application of a bilinear theorem to network sensitivity," *IEEE Trans. Circuit Theory* 12(3):448–450, 1965. Any network function is bilinear in any single element value.
- Fidler, *IEEE TCAS* 23(9):567–571, 1976; Middlebrook et al., "The N extra element theorem," *IEEE TCAS-I* 45(9):919–935, 1998.
- Mohsenizadeh, Keel & Bhattacharyya, IFAC 47(3):6502–6507, 2014, and a Springer chapter, 2016. The same corner result, applied to circuits. (Their 2014 SpringerBrief with DC/AC chapters: partial, not read.)
- Bhattacharyya, Chapellat & Keel, *Robust Control: The Parametric Approach*, 1995, ch. 10. Mapping theorem (Zadeh & Desoer 1963): a multilinear image of a box lies in the convex hull of the corner images.
- For AC and frequency response (corners are not enough):
  - de Gaston & Safonov, *IEEE TAC* 33(2):156–171, 1988: mapping theorem + subdivision.
  - Bartlett, Hollot & Lin, *MCSS* 1(1):61–71, 1988: edge theorem.
  - Hollot & Tempo, *IEEE TAC* 39(2):391–396, 1994: Nyquist envelope.
  - Bartlett, Tesi & Vicino, *IEEE TAC* 38(6):929–933, 1993: frequency response needs edges, not just corners.
  - Bartlett, Tesi & Vicino, *SCL* 19(5):365–370, 1992: corners are insufficient for step response.
  - Fu, *SCL* 15(1):45–52, 1990.
  - Ferber et al., *IJNM* 31(2):e2249, 2018: guaranteed AC bounds over frequency *bands*.
- Dong & Shah, *Fuzzy Sets Syst.* 24(1):65–78, 1987: the vertex method.

## 5. Guaranteed DC operating points (nonlinear)

- **[Neu89]** Neumaier, *J. Math. Anal. Appl.* 144(1):16–25, 1989; and a chapter in *Reliability in Computing*, 1988, pp. 269–286. Enclosing solutions over a parameter box.
- **[Rum90]** Rump, *Math. Comp.* 54(190):721–736, 1990.
- Kolev & Nenov, *Reliable Computing* 7(5):399–408, 2001. Nakaya, Nishi, Oishi & Claus, *JJIAM* 26:327–336, 2009.
- Kolev, *IEEE TCAS-I* 47(5):675–683, 2000.
- Ratschek & Rokne, *JOGO* 3(4):501–518, 1993. Ebers–Moll benchmark: a good regression test.
- Yamamura's linear-programming methods for finding all DC solutions:
  - Yamamura, Kawata & Tokue, *BIT* 38(1):186–199, 1998 (the "LP test").
  - Yamamura & Fujioka, *JCAM* 152:587–595, 2003.
  - Yamamura, Suda & Tamura, "LP narrowing," *Appl. Math. Comput.* 215(1):405–413, 2009.
  - Yamamura & Suda, *IEICE* E92-A(2):638–642, 2009.
  - With tolerances: Yamamura & Haga, *JCSC* 17(5):785–796, 2008; Yamamura, *JCAM* 372:112616, 2020; Pastore, *IJCTA* 44(3):639–659, 2016.
  - (Correction: there is no Yamamura TCAS-I paper on all DC solutions of *transistor* circuits via LP; that one is in IEICE E86-A, 2003.)
- Contractors (HC4 and relatives): Benhamou et al., ICLP 1999, pp. 230–244 (origin of HC4); Chabert & Jaulin, *Artif. Intell.* 173(11):1079–1100, 2009; Araya, Trombettoni & Neveu, AAAI 2010, pp. 9–14 (Mohc: exact on monotone functions).
- Puget & Van Hentenryck, *JOGO* 13(1):75–93, 1998.

## 6. "Find values such that every tolerance passes" (∃∀)

- Goldsztejn, *Reliable Computing* 11(6):443–478, 2005; ACM SAC 2006, pp. 1650–1654.
- Goldsztejn, Michel & Rueher, *Constraints* 14(1):117–135, 2009. Handles "∃x ∀p: g ≤ 0", the design-centering form.
- Shary, *Reliable Computing* 8(5):321–418, 2002, and *Math. Comput. Simul.* 39:53–85, 1995: tolerable solution sets. Parametric version: Popova, *SIMAX* 33(4):1172–1189, 2012.
- Kong, Solar-Lezama & Gao, "Delta-decision procedures for exists-forall problems over the reals," CAV 2018, pp. 219–235. doi:10.1007/978-3-319-96142-2_15. Benchmarks aren't circuits.
- Circuit applications are thin: Lemke et al. 2002 (§2); Spagnuolo, *COMPEL* 25(4):964–978, 2006; Liu et al., ASP-DAC 2007, pp. 203–208 (not rigorous).

## 7. Sensitivities in circuit simulators

- **[DR69]** Director & Rohrer, "The generalized adjoint network and network sensitivities," *IEEE Trans. Circuit Theory* CT-16:318–323, 1969. One transposed solve gives one output's sensitivity to every part.
- **[RNM71]** Rohrer, Nagel, Meyer & Weber, "Computationally efficient electronic-circuit noise calculations," *IEEE JSSC* 6(4):204–213, 1971. SPICE noise analysis is itself an adjoint solve, so building the AC adjoint gets both.
- Hocevar, Yang, Trick & Epler, "Transient sensitivity computation for MOSFET circuits," *IEEE TCAD* 4(4):609–620, 1985. Compares direct and adjoint for transient; prefers direct.
- Cao, Li, Petzold & Serban, "Adjoint sensitivity analysis for differential-algebraic equations: the adjoint DAE system and its numerical solution," *SIAM J. Sci. Comput.* 24(3):1076–1089, 2003.
- **[Xyce16]** Keiter, Swiler, Russo & Wilcox, "Sensitivity Analysis in Xyce," Sandia SAND2016-9437, 2016 (osti.gov/biblio/1562422).
  - DC and transient, direct and adjoint, derived per integrator (BE, trapezoid, BDF2).
  - The adjoint needs stored forward data, and each point-in-time objective needs its own backward pass.
  - Device derivatives: finite differences, automatic differentiation (Sacado), or the ADMS model compiler.
- Ilievski et al., "Adjoint transient sensitivity analysis in circuit simulation," SCEE 2006, Springer 2007, pp. 183–189.
- Meir & Roychowdhury, "BLAST," DAC 2012. Delay sensitivities with a cheaper transient adjoint.
- Liu & Feldmann, "A time-unrolling method to compute sensitivity of dynamic systems," DAC 2014. Reuses the forward integrator code for the adjoint.
- Galán, Feehery & Barton, *Appl. Numer. Math.* 31(1):17–47, 1999. Sensitivities at events; for a threshold crossing, dt*/dp = −(∂x/∂p)/ẋ at t*.
- Periodic steady state: Aprille & Trick, *Proc. IEEE* 60:108–114, 1972; Sarpe, Klaedtke & De Gersem, arXiv:2405.19048, 2024 (preprint).
- Memory for the backward pass: Griewank & Walther, "Algorithm 799: revolve," *ACM TOMS* 26(1):19–45, 2000; SUNDIALS IDAS docs.
- Davis & Palamadai Natarajan, "Algorithm 907: KLU," *ACM TOMS* 37(3), art. 36, 2010. Reference KLU has `klu_tsolve`; our port doesn't yet.

## 8. Worst-case analysis and design centering

- **[AGW94]** Antreich, Graeb & Wieser, "Circuit analysis and optimization driven by worst-case distances," *IEEE TCAD* 13(1):57–71, 1994. The worst-case point, its distance β in σ units, and yield ≈ Φ(β) per spec.
- **[Graeb07]** Graeb, *Analog Design Centering and Sizing*, Springer 2007, doi:10.1007/978-1-4020-6004-5. Design / range / statistical parameters; classical, realistic and general worst-case analysis. (Definitions confirmed verbatim in Graeb's TUM lecture textbook, which follows this book: range parameters are "supply voltage and temperature … lifetime", with acceptance required for all of them.)
- Graeb, Wieser & Antreich, DAC 1993, pp. 142–147. Operating (range) tolerances in worst-case analysis. (Confirmed via citation.)
- Schenkel et al., DAC 2001. Linearize each spec at its own worst point, not the nominal.
- Cijan, Tuma & Bűrmen, *J. Circuits Syst. Comput.* 18(7):1185–1204, 2009. Derivative-free worst-case search; clear statement of the range/statistical split.
- Mukherjee, Carley & Rutenbar, *IEEE TCAD* 19(8):825–839, 2000. (Correction: vol. 19, not 18.) "For every point in the operating range" as infinite programming.
- Director & Hachtel, "The simplicial approximation approach to design centering," *IEEE TCAS* CAS-24:363–372, 1977.
- Brayton, Hachtel & Sangiovanni-Vincentelli, "A survey of optimization techniques for integrated-circuit design," *Proc. IEEE* 69:1334–1362, 1981.
- Bandler, Liu & Tromp, *IEEE TCAS* 23:155–165, 1976. Also (confirmed via citation): Dharchoudhury & Kang, *IEEE TCAD* 14:481–492, 1995; Tahim & Spence, *IEEE TCAS* 26:768–774, 1979.

## 9. Yield estimation beyond plain Monte Carlo

- Dolecek, Qazi, Shah & Chandrakasan, ICCAD 2008, pp. 322–329. The importance-sampling center is the same point as the worst-case point, so one search serves both.
- Kanj, Joshi & Nassif, "Mixture importance sampling," DAC 2006, pp. 69–72.
- Singhee & Rutenbar, "Statistical blockade," *IEEE TCAD* 28(8):1176–1189, 2009.
- Sun et al., scaled-sigma sampling, *IEEE TCAD* 34(7):1096–1109, 2015.
- Polynomial chaos: Manfredi et al., *IEEE MWCL* 25(8):505–507, 2015, and *IEEE TCAS-I* 2014 (partial: volume/pages); Zhang et al., *IEEE TCAD* 32(10):1533–1545, 2013.
- Keiter, Swiler & Wilcox, "Gradient-enhanced polynomial chaos methods for circuit simulation," SCEE 2016, Springer 2018. Uses adjoint gradients to cut the number of runs; fits our sensitivity-first plan.
- Surrogates: Li, Le & Pileggi, *Found. Trends EDA* 1(4):331–480, 2006; Li, *IEEE TCAD* 29(11):1661–1668, 2010; Wang et al., *IEEE TCAD* 37(10):1929–1942, 2018; Gao & Boning, arXiv:2304.09723.
- What a finite number of runs proves:
  - Hanley & Lippman-Hand, *JAMA* 249:1743–1745, 1983: rule of three (0 failures in N runs → failure rate ≤ 3/N at 95%, one-sided; two-sided Clopper–Pearson needs 368 runs for < 1%). Valid only for i.i.d. samples from the assumed production distribution.
  - Wilks, *Ann. Math. Stat.* 12:91–96, 1941: 59 runs give a one-sided 95/95 limit.
  - Campi & Garatti, *SIAM J. Optim.* 19:1211–1230, 2008: the scenario approach.

## 10. Formal methods for analog transient

- Ahmadyan & Vasudevan, "Reachability analysis of nonlinear analog circuits through iterative reachable set reduction," DATE 2013, pp. 1436–1441. (Correction: not Althoff, not DAC 2012; uses polytopes, not zonotopes.)
- Althoff, Yaldiz, Rajhans, Li, Krogh & Pileggi, "Formal verification of phase-locked loops using reachability analysis and continuization," ICCAD 2011, pp. 659–666.
- Lee, Althoff, Hoelldampf, Olbrich & Barke, ASP-DAC 2015, pp. 725–730.
- Althoff & Krogh, "Reachability analysis of nonlinear differential-algebraic systems," *IEEE TAC* 59(2):371–383, 2014.
- Frehse, Krogh & Rutenbar, DATE 2006.
- Covering a region with simulations: Donzé & Maler, HSCC 2007; Duggirala, Mitra & Viswanathan, EMSOFT 2013; Fan et al., arXiv:1803.02975.
- Barke et al., "Formal approaches to analog circuit verification," DATE 2009, pp. 724–729. Plain intervals "very pessimistic"; affine usable for AC.
- Zaki, Tahar & Bois, survey, *Microelectronics J.* 39:1395–1404, 2008.
- Verified integration: Nedialkov, VNODE-LP, 2006; Kapela et al., CAPD, *CNSNS* 101:105578, 2021; Chen, Ábrahám & Sankaranarayanan, Flow*, CAV 2013. These use explicit Taylor methods, which handle stiff circuits poorly (secondary source only), which is why v1 rejected this as the main path.

## 11. Convex / geometric programming for sizing

- Hershenson, Boyd & Lee, "Optimal design of a CMOS op-amp via geometric programming," *IEEE TCAD* 20(1):1–21, 2001.
- Boyd, Kim, Vandenberghe & Hassibi, "A tutorial on geometric programming," *Optim. Eng.* 8:67–127, 2007. Log-space variables make ±% tolerances additive.
- Hsiung, Kim & Boyd, "Tractable approximate robust geometric programming," *Optim. Eng.* 9:95–118, 2008.
- Xu et al., "OPERA," DAC 2005, pp. 632–637. Robust GP with a guaranteed yield bound.
- Saab, Burnell & Hoburg, arXiv:1808.07192, 2018.

## 12. Real component tolerances

- **[ECSS11]** ECSS-Q-HB-30-01A, *Worst case analysis*, 14 Jan 2011 (escies.org).
  - Initial tolerance is random; aging and temperature are "biased (sometimes random)"; supply/interface variation is "generally random".
  - Four methods: EVA (extreme value, "the best initial approach"), EVA combined (named, no formula, "strictly valid only for Gaussian variables"), RSS (three-sigma limits), and Monte Carlo (P = 99.5% at 95% confidence). Analysis is at end of life.
  - **Not in ECSS:** the "biased linear + random RSS" rule. That is RAC worst-case circuit analysis practice.
  - ECSS has since discontinued the handbook; its requirements moved to ECSS-Q-ST-30C.
- OrCAD PSpice Reference Guide. DEV tolerances vary parts independently, LOT tolerances vary parts sharing a model together. Default distribution is UNIFORM; **with GAUSS the tolerance means 1σ**, not 3σ.
- **[Vishay CRCW]** Vishay D/CRCW e3 datasheet, doc 20035. ±1%, ±100 ppm/K; ΔR/R ≤ 1% after 1000 h and ≤ 2% after 8000 h at rated power and 70 °C (a stress test of ~11 months at full power). No 10-year figure is given; the datasheet says there is no limited lifetime.
- Vishay MORN/ORN thin-film networks. Ratio tolerance down to ±0.01% vs ±0.05–1% absolute; TCR tracking ±1–5 ppm/°C.
- **[Maxim 5527]** Fortunato, Maxim Tutorial 5527, "Temperature and voltage variation of ceramic capacitors, or why your 4.7 µF capacitor becomes a 0.33 µF capacitor." The 0.33 µF case is a 6.3 V Y5V 0603 part at 5 V (typical −92.9%), which the author says he never uses; an X7R 0805 at 12 V gives 1.53 µF. Author: Mark Fortunato, Dec 2012.
- KEMET and TDK aging notes: X7R loses about 2.5% per decade-hour.
- AEC-Q200 Rev E, 2023. A stress-qualification standard, not a tolerance definition.
- IEC 60063:2015. E-series values are spaced geometrically (log space).
- Spence & Soin, *Tolerance Design of Electronic Circuits*, Addison-Wesley, 1988 (catalog entry only).
- The "1% parts are culled from 5%" story: only informal evidence (LambdaFox, 2022, 1000 parts measured). Modern parts measured Gaussian; a 5% batch sat at −1.68% offset with σ ≈ 0.08%. **The real hazard is a narrow-but-offset reel**, which breaks independence.

## 13. Tools studied

- atopile: https://github.com/atopile/atopile (cloned in `externals/atopile`).
- Spade: https://gitlab.com/spade-lang/spade (cloned in `externals/spade`).
- kv library (Kashiwagi): the AA implementation reference for [R&K15].
- IBEX: https://github.com/ibex-team/ibex-lib. dReal4: https://github.com/dreal/dreal4.

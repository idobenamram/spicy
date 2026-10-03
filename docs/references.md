# References

The projects whose source we read to ground a design decision, and the version of each that we read. A citation in a doc or a comment names the project and the item. This table gives the version, so a reader can open the same source. How to cite is in [`writing_docs.md`](writing_docs.md#references).

`scripts/fetch_references.sh` copies each project into `externals/<key>` (ignored by git), at the version in the table. When the "Read" column names folders, only those folders are copied.

<a name="table"></a>
## The references

| Key | Project | Version | Repository | Read | Copying our code from it |
|---|---|---|---|---|---|
| `rustc` | The Rust compiler | `1.98.1` | https://github.com/rust-lang/rust.git | `compiler` | Allowed with a notice (MIT or Apache-2.0) |
| `rustc-dev-guide` | The rustc dev guide | `33dcb0c792207976cb9ba2118c676d29e01400ee` | https://github.com/rust-lang/rustc-dev-guide.git | | Allowed with a notice (MIT or Apache-2.0) |
| `rust-analyzer` | rust-analyzer | `1ad44dc58e65304b594063e70c144ecb58643671` | https://github.com/rust-lang/rust-analyzer.git | | Allowed with a notice (MIT or Apache-2.0) |
| `zig` | The Zig compiler | `0.17.0` | https://codeberg.org/ziglang/zig.git | `src lib/std` | Allowed with a notice (MIT) |
| `cranelift` | Cranelift, in Wasmtime | `48fc50b803bf0af1fee109f4ac86cf0bf88515de` | https://github.com/bytecodealliance/wasmtime.git | `cranelift/codegen/src/ir cranelift/entity` | Allowed with a notice (Apache-2.0 with the LLVM exception) |
| `llvm` | MLIR, in the LLVM project | `2dc2a1f1b94f1953f0ef52bd748fb1f0c07cd780` | https://github.com/llvm/llvm-project.git | `mlir/docs` | Allowed with a notice (Apache-2.0 with the LLVM exception) |
| `ngspice` | ngspice | `ngspice-42` | https://git.code.sf.net/p/ngspice/ngspice | | Most files: allowed with a notice (Modified BSD). Some folders have other licenses: read `COPYING` first |
| `xyce` | Xyce | `6243c628a36dbf6431424722d7f5265e22752c4a` | https://github.com/Xyce/Xyce.git | | Never (GPL-3.0) |
| `gnucap` | Gnucap | `20240220` | https://git.savannah.gnu.org/git/gnucap.git | | Never (GPL-3.0) |
| `openvaf` | OpenVAF (OSDI) | `a9697ae7780518f021f9f64e819b3a57033bd39f` | https://github.com/pascalkuthe/OpenVAF.git | | Never (GPL-3.0) |
| `vacask` | VACASK | `55736a59fa955429575b4aab050525a306eafba3` | https://codeberg.org/arpadbuermen/VACASK.git | | Never (AGPL-3.0) |
| `spade` | Spade | `177e5c46058a50d51659a487916ffefae8ac23c9` | https://gitlab.com/spade-lang/spade.git | | Never for the compiler (EUPL-1.2). The standard library, `spade-compiler/stdlib`: allowed with a notice (MIT or Apache-2.0) |
| `atopile` | atopile | `619eda7f777558a3e500dbad9cc2941712881495` | https://github.com/atopile/atopile.git | | Allowed with a notice (MIT) |

"Allowed with a notice" means: copy only with the project's license text and a line in `THIRD_PARTY_NOTICES.md`. "Never" means: read it and learn from it, but write our own code.

<a name="notes"></a>
## Notes

- **ngspice.** `ngspice-42` is the version of the installed `ngspice` binary, so the source matches what we run. Before 2026-10-03, the copy on disk was the later commit `56a152c` (2026-09-25), and its line numbers can differ: `translate_inst_name()` starts at line 1133 in `ngspice-42` and at line 1131 in `56a152c`. Check an older line citation against `ngspice-42` before you rely on it.
- **Spade.** Its compiler is under EUPL-1.2, a copyleft license. We can read it, learn from it and cite it, but copied code would have to stay under EUPL or a license in its appendix, and MIT is not one of them. Its rule against AI-generated content covers contributions *to* Spade, not reading it.
- **A new reference.** Add a row with a pinned version (a release tag, or a full commit hash), then run `scripts/fetch_references.sh`.
- **A newer version.** Change the row only when you read the newer source. Old citations still name items that a reader can find with a search.

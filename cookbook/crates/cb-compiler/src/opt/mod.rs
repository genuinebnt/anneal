//! The optimiser: a pass manager over [`MirProgram`].
//!
//! Every pass is a function `&mut MirProgram -> changed?`. [`optimize`] runs
//! the pipeline of an `--opt-level`, verifies the MIR after *each* pass (so a
//! buggy pass is named in the error) and hands the MIR to a dump callback for
//! `--dump-after=<pass>`.

pub mod cfg;
pub mod cse;
pub mod dce;
pub mod dom;
pub mod gvn;
pub mod inline;
pub mod licm;
pub mod live;
pub mod loops;
pub mod prop;
pub mod sccp;
pub mod ssa;
pub mod strength;

use crate::mir::{MirProgram, verify};

/// Every pass name accepted by `--dump-after` and [`run_pass`].
pub const PASS_NAMES: &[&str] = &[
    "simplify-cfg",
    "const-prop",
    "copy-prop",
    "cse",
    "strength",
    "dce",
    "inline",
    "tco",
    "bce",
    "ssa",
    "sccp",
    "gvn",
    "licm",
    "out-of-ssa",
];

/// Passes that only make sense on SSA functions (`ssa` creates the form, `out-of-ssa` removes it).
pub const SSA_ONLY: &[&str] = &["sccp", "gvn", "licm", "out-of-ssa"];

fn per_func(p: &mut MirProgram, f: impl Fn(&mut crate::mir::Func, u32) -> bool) -> bool {
    let mut changed = false;
    for i in 0..p.funcs.len() {
        changed |= f(&mut p.funcs[i], i as u32);
    }
    changed
}

/// Run one pass by name. Returns whether it changed the program, or `None` for an unknown name.
pub fn run_pass(name: &str, p: &mut MirProgram) -> Option<bool> {
    Some(match name {
        "simplify-cfg" => per_func(p, |f, _| cfg::simplify_cfg(f)),
        "const-prop" => per_func(p, |f, _| {
            prop::propagate(
                f,
                prop::Mode {
                    consts: true,
                    copies: false,
                },
            )
        }),
        "copy-prop" => per_func(p, |f, _| {
            prop::propagate(
                f,
                prop::Mode {
                    consts: false,
                    copies: true,
                },
            )
        }),
        "cse" => per_func(p, |f, _| cse::cse(f)),
        "strength" => per_func(p, |f, _| strength::strength(f)),
        "dce" => per_func(p, |f, _| dce::dce(f)),
        "tco" => per_func(p, loops::tail_calls),
        "bce" => per_func(p, |f, _| loops::bounds_checks(f)),
        "inline" => inline::inline(p),
        "ssa" => per_func(p, |f, _| ssa::to_ssa(f)),
        "sccp" => per_func(p, |f, _| sccp::sccp(f)),
        "gvn" => per_func(p, |f, _| gvn::gvn(f)),
        "licm" => per_func(p, |f, _| licm::licm(f)),
        "out-of-ssa" => per_func(p, |f, _| ssa::from_ssa(f)),
        _ => return None,
    })
}

/// The pass sequence for an optimisation level (0 = none, 3 = everything).
pub fn pipeline(level: u8) -> Vec<&'static str> {
    let scalar = [
        "simplify-cfg",
        "const-prop",
        "copy-prop",
        "dce",
        "simplify-cfg",
    ];
    let l2 = [
        "simplify-cfg",
        "const-prop",
        "copy-prop",
        "cse",
        "copy-prop",
        "strength",
        "const-prop",
        "dce",
        "simplify-cfg",
        "tco",
        "bce",
        "dce",
        "simplify-cfg",
    ];
    match level {
        0 => vec![],
        1 => scalar.to_vec(),
        2 => l2.to_vec(),
        _ => ["inline", "simplify-cfg"]
            .iter()
            .chain(l2.iter())
            .chain(["ssa", "sccp", "gvn", "licm", "out-of-ssa"].iter())
            .chain(["const-prop", "copy-prop", "cse", "dce", "simplify-cfg"].iter())
            .copied()
            .collect(),
    }
}

#[derive(Clone, Debug, Default)]
pub struct OptOptions {
    pub level: u8,
    /// Dump the MIR after each run of these passes.
    pub dump_after: Vec<String>,
    /// Verify after every pass (default for tests).
    pub verify: bool,
}

/// Run an explicit pass list; `dump` receives `(pass, program)` for dumped passes.
pub fn run_passes(
    p: &mut MirProgram,
    passes: &[&str],
    opts: &OptOptions,
    dump: &mut dyn FnMut(&str, &MirProgram),
) -> Result<(), String> {
    for name in passes {
        // SSA functions carry phis that the register-form passes do not understand.
        let want_ssa = SSA_ONLY.contains(name);
        if let Some(f) = p
            .funcs
            .iter()
            .find(|f| f.ssa != want_ssa && !(*name == "ssa" && f.ssa))
        {
            return Err(format!(
                "pass `{name}` cannot run on `{}`: it {} in SSA form",
                f.inst_name,
                if want_ssa { "is not" } else { "is" }
            ));
        }
        run_pass(name, p).ok_or_else(|| format!("unknown pass `{name}`"))?;
        if opts.verify
            && let Err(errs) = verify::verify(p)
        {
            return Err(format!(
                "MIR verification failed after pass `{name}`:\n{}",
                errs.join("\n")
            ));
        }
        if opts.dump_after.iter().any(|d| d == name || d == "all") {
            dump(name, p);
        }
    }
    Ok(())
}

pub fn optimize(
    p: &mut MirProgram,
    opts: &OptOptions,
    dump: &mut dyn FnMut(&str, &MirProgram),
) -> Result<(), String> {
    run_passes(p, &pipeline(opts.level), opts, dump)
}

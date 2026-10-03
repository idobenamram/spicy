//! Pass 7 for each root (contracts_plan.md step 5, §0.3, §2.3, §2.8): its contract's
//! default setup, flattened as a device's fields are. A field with a spread becomes a
//! Range knob, the world the design must work in everywhere, not a part's random
//! variation (E16); a point stays fixed; a field not written stays unset (an ideal
//! source, no load). The knobs come after the part knobs, in port order, then each
//! shape's field order, then `temp`, so a setup renumbers no part knob.

use super::{Knobs, RootFlattener};
use crate::design::{BlockId, PortId, Temp};
use crate::flat::{FlatField, FlatPortSetup, FlatSetup, KnobSource};

impl RootFlattener<'_> {
    /// `root`'s default setup, flattened, its knobs added to `knobs`. `None` when the
    /// root has no contract or its `setup = S;` didn't resolve.
    pub(super) fn setup(&self, root: BlockId, knobs: &mut Knobs) -> Option<FlatSetup> {
        let contract = self.design.contracts[root.index()].as_ref()?;
        let id = contract.default_setup.ok()?;
        let setup = &self.design.setups[id.index()];
        let ports = (setup.ports.iter().enumerate())
            .map(|(port, entry)| {
                let entry = entry.as_ref()?;
                let port = PortId::new(port);
                let fields = (entry.fields.iter().enumerate())
                    .map(|(field, &value)| {
                        knobs.field(value, KnobSource::SetupField { port, field })
                    })
                    .collect();
                Some(FlatPortSetup {
                    shape: entry.shape,
                    fields,
                })
            })
            .collect();
        // `temp: ambient` is the env's knob; `temp: -40°C..=125°C` the setup's own.
        let temp = match setup.temp {
            Ok(Temp::Env(env)) => match self.design.envs[env.index()].value {
                Ok(value) => knobs.given(value, KnobSource::Env(env)),
                Err(reported) => FlatField::Invalid(reported),
            },
            Ok(Temp::Value(value)) => knobs.given(value, KnobSource::SetupTemp),
            Err(reported) => FlatField::Invalid(reported),
        };
        Some(FlatSetup {
            setup: id,
            ports,
            temp,
        })
    }
}

//! Load-phase declarative villager type registration.

pub trait Hooks {
    /// Declare villager types once per mod load, before world admission.
    /// The engine validates the declarations together before publishing the registry.
    ///
    /// Native only: the villager type registrar has no WebAssembly adapter.
    #[hook(
        side = "server",
        contract = "collect",
        out = "types",
        context = "registration",
        neoforge = "RegisterEvent"
    )]
    fn register_villager_types();
}

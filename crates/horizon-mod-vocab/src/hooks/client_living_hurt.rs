//! Client-side observation of a living entity's hurt effect.

pub trait Hooks {
    /// The client applied a server damage notice to `entity`, after its hurt animation
    /// was updated (`ClientPacketListener.handleDamageEvent`). `causing_entity` and
    /// `direct_entity` name the attacker and the immediate source (such as an arrow)
    /// when the client can resolve them. Missing sources are `None`. This is one notice
    /// per applied damage event, including multiple events in one tick; a separate
    /// hurt-tilt animation packet does not fire it. It cannot modify the damage.
    #[hook(side = "client", contract = "listen", cancellable = false)]
    fn client_living_hurt();
}

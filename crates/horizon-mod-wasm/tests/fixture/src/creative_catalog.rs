//! The same creative catalog consumer runs as native code and inside the compiled fixture.
use std::sync::atomic::{AtomicBool, Ordering};

use mod_api::{
    BroadcastMessageEvent, BuildCreativeModeTabContentsEvent, ClientHost, CreativeModeTabs,
    CreativeTabEdit, EditCreativeModeTabContentsEvent, Flow, Host, Identifier, ItemStack,
    LogicalSide, ServerHost, Text,
};

pub const PACKAGE: &str = "creative_catalog_probe";
pub const TAB: &str = "creative_catalog_probe:m_specimens";
pub const CONTROL: &str = "minecraft:building_blocks";
pub const PING: &str = "creative_catalog_probe:verify";
pub const ACK: &str = "creative_catalog_probe:complete";
const SPECIAL: [&str; 3] = [
    "minecraft:search",
    "minecraft:hotbar",
    "minecraft:inventory",
];
const ICON: &str =
    r#"{id:"minecraft:stone",components:{"minecraft:custom_name":{"text":"Specimen"}}}"#;

static VERIFIED: AtomicBool = AtomicBool::new(false);
static LEGACY_CALLED: AtomicBool = AtomicBool::new(false);

fn id(value: &str) -> Identifier {
    Identifier::parse(value).unwrap()
}

fn item(host: &dyn Host, value: &str) -> ItemStack {
    ItemStack::of(host.item_key(&id(value)).unwrap())
}

fn named(host: &dyn Host, name: &str) -> ItemStack {
    // Unique component variants: these are not already in vanilla search just because stone is.
    host.item_from_snbt(&format!(
        r#"{{id:"minecraft:stone",components:{{"minecraft:custom_name":{{"text":"{name}"}}}}}}"#
    ))
    .unwrap()
}

fn check_headers(host: &dyn Host, tabs: &CreativeModeTabs) {
    let custom: Vec<_> = tabs
        .tabs
        .iter()
        .filter(|tab| tab.id.namespace() == PACKAGE)
        .map(|tab| tab.id.to_string())
        .collect();
    assert_eq!(
        custom,
        [
            "creative_catalog_probe:z_first",
            TAB,
            "creative_catalog_probe:a_last",
        ],
        "the package's before/after edges override filename order"
    );
    let specimens = tabs.tab(&id(TAB)).unwrap();
    assert_eq!(specimens.display_name, "Creative specimens");
    assert_eq!(specimens.icon, host.item_from_snbt(ICON).unwrap());
    for key in SPECIAL {
        assert!(tabs.tab(&id(key)).is_some(), "missing special tab {key}");
    }
}

pub fn edit(host: &dyn Host, event: &mut EditCreativeModeTabContentsEvent<'_>) {
    if !host.is_loaded(PACKAGE) {
        return;
    }
    let tab = event.tab.to_string();
    if tab == "minecraft:search" {
        event.edits.extend([
            CreativeTabEdit::InsertAfter {
                anchor: named(host, "Creative after"),
                stack: named(host, "minecraft:search"),
            },
            CreativeTabEdit::Remove {
                stack: named(host, "Creative before"),
            },
            CreativeTabEdit::Append {
                stack: item(host, "minecraft:command_block"),
            },
        ]);
        return;
    }
    if SPECIAL.contains(&tab.as_str()) {
        event.edits.push(CreativeTabEdit::Append {
            stack: named(host, &tab),
        });
        return;
    }
    if tab != TAB {
        return;
    }
    assert!(!event.has_permissions);
    let catalog = host.creative_tabs().unwrap();
    check_headers(host, &catalog);
    let initial = [
        item(host, "minecraft:flint"),
        item(host, "minecraft:emerald"),
        item(host, "minecraft:stick"),
    ];
    let original = catalog.tab(event.tab).unwrap();
    assert_eq!(original.display_items, initial);
    assert!(
        catalog
            .tabs_showing(&initial[0])
            .any(|tab| tab.id == original.id)
    );
    for key in ["minecraft:hotbar", "minecraft:inventory"] {
        assert!(
            catalog.tab(&id(key)).unwrap().display_items.is_empty(),
            "personal slots are not catalog contents"
        );
    }

    let side = match event.side {
        LogicalSide::Client => "client",
        LogicalSide::Server => "server",
    };
    let removed = named(host, "Creative transient");
    let mut emerald = initial[1].clone();
    emerald.set_count(2).unwrap();
    event.edits.extend([
        CreativeTabEdit::InsertBefore {
            anchor: emerald,
            stack: named(host, "Creative before"),
        },
        CreativeTabEdit::InsertAfter {
            anchor: initial[1].clone(),
            stack: named(host, "Creative after"),
        },
        CreativeTabEdit::InsertBefore {
            anchor: item(host, "minecraft:stone"),
            stack: named(host, "Creative wrong anchor"),
        },
        CreativeTabEdit::Append {
            stack: named(host, "Creative after"),
        },
        CreativeTabEdit::Remove {
            stack: initial[0].clone(),
        },
        CreativeTabEdit::Append {
            stack: named(host, side),
        },
        CreativeTabEdit::Append {
            stack: removed.clone(),
        },
        CreativeTabEdit::Remove { stack: removed },
    ]);
}

pub fn legacy(host: &dyn ClientHost, event: &mut BuildCreativeModeTabContentsEvent<'_>) {
    if !host.is_loaded(PACKAGE) {
        return;
    }
    LEGACY_CALLED.store(true, Ordering::Relaxed);
    assert!(
        !SPECIAL.contains(&event.tab),
        "legacy hook ran on a special tab"
    );
    assert!(!event.has_permissions);
    if event.tab == TAB || event.tab == CONTROL {
        event.items.push(named(host, "Creative legacy"));
    }
}

fn check_built(host: &dyn Host, client: bool) {
    let catalog = host.built_creative_tabs().unwrap();
    check_headers(host, &catalog);
    let specimens = catalog.tab(&id(TAB)).unwrap();
    let legacy = named(host, "Creative legacy");
    let expected = [
        named(host, "Creative before"),
        item(host, "minecraft:emerald"),
        named(host, "Creative after"),
        item(host, "minecraft:stick"),
        named(host, if client { "client" } else { "server" }),
    ];
    assert_eq!(
        specimens
            .display_items
            .iter()
            .filter(|stack| **stack != legacy)
            .collect::<Vec<_>>(),
        expected.iter().collect::<Vec<_>>()
    );
    assert!(
        catalog
            .tabs_showing(&expected[0])
            .any(|tab| tab.id == specimens.id)
    );
    assert!(
        !catalog
            .tabs_showing(&item(host, "minecraft:flint"))
            .any(|tab| tab.id == specimens.id)
    );

    let control = catalog.tab(&id(CONTROL)).unwrap();
    assert!(
        control
            .display_items
            .contains(&item(host, "minecraft:stone"))
    );
    assert_eq!(control.display_items.contains(&legacy), client);
    assert_eq!(specimens.display_items.contains(&legacy), client);
    assert_eq!(LEGACY_CALLED.load(Ordering::Relaxed), client);
    for key in SPECIAL {
        let tab = catalog.tab(&id(key)).unwrap();
        if key == "minecraft:search" {
            let anchor = named(host, "Creative after");
            let inserted = named(host, key);
            assert!(
                tab.display_items
                    .windows(2)
                    .any(|pair| pair[0] == anchor && pair[1] == inserted)
            );
            assert!(!tab.display_items.contains(&named(host, "Creative before")));
            assert!(
                !tab.display_items
                    .contains(&item(host, "minecraft:command_block"))
            );
        }
        assert!(tab.display_items.contains(&named(host, key)), "{key}");
        if key != "minecraft:search" {
            assert_eq!(tab.display_items.len(), 1, "{key}");
        }
    }
    assert!(catalog.tab(&id("creative_catalog_probe:absent")).is_none());
}

pub fn client_tick(host: &dyn ClientHost) {
    if !host.is_loaded(PACKAGE) {
        return;
    }
    check_built(host, true);
    let search = host.creative_search_items();
    let snapshot = host.built_creative_tabs().unwrap();
    assert_eq!(
        search,
        snapshot.tab(&id("minecraft:search")).unwrap().display_items
    );
    for name in [
        "Creative after",
        "client",
        "Creative legacy",
        "minecraft:search",
    ] {
        let expected = named(host, name);
        assert_eq!(search.iter().filter(|stack| **stack == expected).count(), 1);
    }
    assert!(!search.contains(&named(host, "Creative before")));
    assert!(!search.contains(&named(host, "Creative transient")));
    assert!(
        search.contains(&item(host, "minecraft:flint")),
        "vanilla search entries remain"
    );
    VERIFIED.store(true, Ordering::Relaxed);
}

pub fn server_started(host: &dyn ServerHost) {
    if host.is_loaded(PACKAGE) {
        check_built(host, false);
        VERIFIED.store(true, Ordering::Relaxed);
    }
}

pub fn acknowledge(host: &dyn Host, event: &mut BroadcastMessageEvent<'_>) -> Flow {
    if host.is_loaded(PACKAGE) && *event.line == Text::literal(PING) {
        assert!(
            VERIFIED.load(Ordering::Relaxed),
            "catalog assertions did not complete"
        );
        *event.line = Text::literal(ACK);
    }
    Flow::Continue
}

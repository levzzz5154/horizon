//! The creative catalog shared by the client and server.
use crate::{Identifier, ItemStack};

/// A snapshot of the creative catalog, in tab display order.
#[derive(Clone, Debug, PartialEq, Eq, horizon_mod_api_macros::Wire)]
#[non_exhaustive]
pub struct CreativeModeTabs {
    /// Visible tabs, including Search, Hotbar and Inventory.
    pub tabs: Vec<CreativeModeTabInfo>,
}

impl CreativeModeTabs {
    /// Wrap the ordered tabs of a catalog snapshot.
    pub fn new(tabs: Vec<CreativeModeTabInfo>) -> Self {
        Self { tabs }
    }

    /// Find a visible tab by its registry identifier.
    pub fn tab(&self, id: &Identifier) -> Option<&CreativeModeTabInfo> {
        self.tabs.iter().find(|tab| &tab.id == id)
    }

    /// Find all tabs whose display items contain this item and component patch, in tab order.
    /// Stack counts do not take part in this comparison.
    pub fn tabs_showing<'a>(
        &'a self,
        stack: &'a ItemStack,
    ) -> impl Iterator<Item = &'a CreativeModeTabInfo> {
        self.tabs.iter().filter(move |tab| {
            tab.display_items
                .iter()
                .any(|item| item.is_stackable_with(stack))
        })
    }
}

/// A creative tab and its catalog stacks. Personal inventory slots and saved toolbars are not
/// part of the catalog.
#[derive(Clone, Debug, PartialEq, Eq, horizon_mod_api_macros::Wire)]
#[non_exhaustive]
pub struct CreativeModeTabInfo {
    /// The tab's registry identifier, such as `minecraft:building_blocks`.
    pub id: Identifier,
    /// The tab's declared name, localized on the client.
    pub display_name: String,
    /// The tab's icon stack, with its component patch intact.
    pub icon: ItemStack,
    /// Stacks in display order. Search is the unfiltered search list.
    pub display_items: Vec<ItemStack>,
}

impl CreativeModeTabInfo {
    /// Construct a tab snapshot from its catalog entry.
    pub fn new(
        id: Identifier,
        display_name: String,
        icon: ItemStack,
        display_items: Vec<ItemStack>,
    ) -> Self {
        Self {
            id,
            display_name,
            icon,
            display_items,
        }
    }
}

/// Why a creative catalog could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, horizon_mod_api_macros::Wire)]
#[non_exhaustive]
pub enum CreativeTabReadError {
    /// This host does not have a published creative catalog of the requested kind yet.
    Unavailable,
}

impl crate::NoBackend for CreativeTabReadError {
    fn no_backend() -> Self {
        Self::Unavailable
    }
}

/// An ordered edit to a creative tab. Matching uses the item and its components, not the count.
#[derive(Clone, Debug, PartialEq, Eq, horizon_mod_api_macros::Wire)]
#[non_exhaustive]
pub enum CreativeTabEdit {
    /// Append a stack if it is not already present.
    Append {
        /// The stack to append.
        stack: ItemStack,
    },
    /// Insert before the first exact match. A missing anchor or duplicate insertion is ignored.
    InsertBefore {
        /// The item and components to match.
        anchor: ItemStack,
        /// The stack to insert.
        stack: ItemStack,
    },
    /// Insert after the first exact match. A missing anchor or duplicate insertion is ignored.
    InsertAfter {
        /// The item and components to match.
        anchor: ItemStack,
        /// The stack to insert.
        stack: ItemStack,
    },
    /// Remove every exact match from this tab.
    Remove {
        /// The item and components to match.
        stack: ItemStack,
    },
}

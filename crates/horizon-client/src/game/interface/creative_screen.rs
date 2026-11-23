//! The creative inventory screen (`CreativeModeInventoryScreen`): its tabs, pages, tooltips,
//! trash slot, search box and clicks.
use super::*;
use crate::game::api_values::{TryIntoApi, TryIntoEngine, stack_value};
use horizon_client::creative::CatalogEdit;
use mod_api::{
    CreativeModeTabInfo, CreativeModeTabs, CreativeTabEdit, CreativeTabReadError, Identifier,
    LogicalSide,
};
use std::{convert::Infallible, result::Result};

fn api_stacks(items: &[ItemStack]) -> Vec<mod_api::ItemStack> {
    let mut stacks = Vec::with_capacity(items.len());
    stacks.extend(items.iter().filter_map(|stack| stack.try_into_api().ok()));
    stacks
}

/// Convert the borrowed Search list at the host boundary, without copying the engine stacks.
pub(in crate::game) struct CreativeSearchItems<'a>(&'a [ItemStack]);

impl TryIntoApi for CreativeSearchItems<'_> {
    type Api = Vec<mod_api::ItemStack>;
    type Error = Infallible;

    fn try_into_api(&self) -> Result<Self::Api, Self::Error> {
        Ok(api_stacks(self.0))
    }
}

impl Game<'_> {
    /// `CreativeModeInventoryScreen.hasPermissions` (`canUseGameMasterBlocks`
    /// and the Operator Items Tab option) given to `tryRefreshInvalidatedTabs`,
    /// as the screen's constructor and `containerTick` do. When the tabs need
    /// building, each item tab asks the mods for their stacks
    /// (`BuildCreativeModeTabContentsEvent`) before the build takes them.
    pub(in crate::game) fn refresh_creative_tabs(&mut self) {
        let permissions = self.creative_catalog_permissions();
        let enchantments = Arc::clone(self.item_enchantments());
        self.creative_screen.set_enchantments(enchantments);
        self.update_creative_catalog(permissions, true);
    }

    fn creative_catalog_permissions(&self) -> bool {
        !self.is_logical_server()
            && self.can_use_game_master_blocks()
            && self.settings.operator_items_tab
    }

    /// Publish the catalog on its owning game thread, never inside a host read
    /// (a Wasm guest cannot dispatch to itself recursively).
    pub(in crate::game) fn refresh_creative_catalog(&mut self) {
        self.update_creative_catalog(self.creative_catalog_permissions(), false);
    }

    fn update_creative_catalog(&mut self, permissions: bool, legacy: bool) {
        if self.res.rest.get().is_none() || !self.creative_screen.available() {
            return;
        }
        let features = self
            .remote
            .as_ref()
            .map_or(&self.level.enabled_features, |remote| &remote.features);
        if !Arc::ptr_eq(&self.creative_features, features) {
            if self.creative_features.as_ref() != features.as_ref() {
                self.creative_screen.invalidate_catalog();
            }
            self.creative_features = Arc::clone(features);
        }
        self.creative_screen
            .update_catalog_permissions(permissions, &self.pack.names);
        let needs_edits = self.creative_screen.needs_catalog_build(permissions);
        let legacy_tabs = if legacy {
            self.creative_screen.tabs_to_build(permissions)
        } else {
            None
        };
        if !needs_edits && legacy_tabs.is_none() {
            return;
        }
        let added = legacy_tabs.map(|tabs| {
            tabs.into_iter()
                .filter_map(|tab| {
                    let mut items = Vec::new();
                    self.native_build_creative_mode_tab_contents(&tab, permissions, &mut items);
                    (!items.is_empty()).then_some((tab, items))
                })
                .collect()
        });
        let edits = needs_edits.then(|| self.collect_creative_edits(permissions));
        self.creative_screen
            .build_catalog(permissions, added, edits, &self.pack.names);
    }

    fn collect_creative_edits(&self, permissions: bool) -> Vec<(String, Vec<CatalogEdit>)> {
        if !mod_api::__private::listened(mod_api::HookId::EditCreativeModeTabContents) {
            return Vec::new();
        }
        let side = if self.is_logical_server() {
            LogicalSide::Server
        } else {
            LogicalSide::Client
        };
        let items = self.items();
        self.creative_screen
            .catalog_tab_ids()
            .filter_map(|id| {
                if id == "minecraft:op_blocks" && !permissions {
                    return None;
                }
                let tab = Identifier::parse(id).ok()?;
                let mut supplied = Vec::new();
                self.native_edit_creative_mode_tab_contents(&tab, side, permissions, &mut supplied);
                let insert = |stack: &mod_api::ItemStack| {
                    let stack = stack.try_into_engine(items).ok()?;
                    (stack.item.as_ref() != "minecraft:air"
                        && (permissions || !self.creative_screen.operator_only(&stack)))
                    .then_some(stack)
                };
                let edits: Vec<_> = supplied
                    .iter()
                    .filter_map(|edit| {
                        let anchor = |value| stack_value(value).ok();
                        Some(match edit {
                            CreativeTabEdit::Append { stack } => {
                                CatalogEdit::Append(insert(stack)?)
                            }
                            CreativeTabEdit::InsertBefore { anchor: at, stack } => {
                                CatalogEdit::Before(anchor(at)?, insert(stack)?)
                            }
                            CreativeTabEdit::InsertAfter { anchor: at, stack } => {
                                CatalogEdit::After(anchor(at)?, insert(stack)?)
                            }
                            CreativeTabEdit::Remove { stack } => {
                                CatalogEdit::Remove(anchor(stack)?)
                            }
                            _ => return None,
                        })
                    })
                    .collect();
                (!edits.is_empty()).then(|| (id.to_owned(), edits))
            })
            .collect()
    }

    pub(in crate::game) fn creative_search_items(&self) -> CreativeSearchItems<'_> {
        CreativeSearchItems(self.creative_screen.search_items())
    }

    pub(in crate::game) fn creative_tabs(&self) -> Result<CreativeModeTabs, CreativeTabReadError> {
        match self.creative_screen.base_contents() {
            Some(base) => self.creative_snapshot(&base.tabs, &base.search_items),
            None => self.built_creative_tabs(),
        }
    }

    pub(in crate::game) fn built_creative_tabs(
        &self,
    ) -> Result<CreativeModeTabs, CreativeTabReadError> {
        self.creative_snapshot(
            &self.creative_screen.tabs,
            self.creative_screen.search_items(),
        )
    }

    fn creative_snapshot(
        &self,
        tabs: &[horizon_client::creative::Tab],
        search: &[ItemStack],
    ) -> Result<CreativeModeTabs, CreativeTabReadError> {
        if !self.creative_screen.available() || self.res.rest.get().is_none() {
            return Err(CreativeTabReadError::Unavailable);
        }
        let server = self.is_logical_server();
        let tabs = tabs
            .iter()
            .filter_map(|tab| {
                let id = Identifier::parse(&tab.id).ok()?;
                let icon = tab.icon.try_into_api().ok().or_else(|| {
                    mod_api::__private::item_key("minecraft:stone").map(mod_api::ItemStack::of)
                })?;
                let name = if server {
                    tab.name
                        .as_ref()
                        .map_or_else(|| tab.title.clone(), |(name, _, _)| name.clone())
                } else {
                    self.creative_tab_title(tab)
                };
                let items = if tab.search { search } else { &tab.items };
                Some(CreativeModeTabInfo::new(id, name, icon, api_stacks(items)))
            })
            .collect();
        Ok(CreativeModeTabs::new(tabs))
    }
    pub(super) fn creative_inventory_open(&self) -> bool {
        self.screen == Screen::Creative && self.creative_screen.tab().inventory
    }
    /// An item tab of the creative screen (its ItemPickerMenu).
    pub(super) fn creative_tab_open(&self) -> bool {
        self.screen == Screen::Creative && !self.creative_screen.tab().inventory
    }
    pub(super) fn creative_origin(batch: &HudBatch) -> (f32, f32) {
        batch.centered_origin(Creative::WIDTH, Creative::HEIGHT)
    }
    /// A creative tab's title: `itemGroup.*` for vanilla's; a mod tab's
    /// `itemGroup.<mod>.<name>`, else its `title` as a lang key (a mod's own
    /// `itemGroup.<mod>`), else `title` as English text.
    pub(super) fn creative_tab_title(&self, tab: &horizon_client::creative::Tab) -> String {
        if let Some((name, _, translate)) = &tab.name {
            return if *translate {
                self.translate(name).unwrap_or_else(|| name.clone())
            } else {
                name.clone()
            };
        }
        match horizon_client::creative::item_group_key(&tab.id) {
            Some(key) => self.translate_with_fallback(key, &tab.title),
            None => {
                let own = self.translate_with_fallback(&tab.title, &tab.title);
                self.translate_with_fallback(
                    &format!("itemGroup.{}", tab.id.replace(':', ".")),
                    &own,
                )
            }
        }
    }
    /// Vanilla tooltip: dark box with a purple gradient border, laid out and
    /// drawn like an item tooltip (4 px of padding on every side).
    pub(super) fn tooltip(&self, batch: &mut HudBatch, text: &str, x: f32, y: f32) {
        let atlas = &self.gui.atlas;
        let lines = vec![(horizon_client::text::Text::literal(text), u32::MAX)];
        let screen = (batch.width, batch.height);
        let Some(at) = self.tooltip_pre(batch, None, &lines, &[], (x, y), screen) else {
            return;
        };
        let (x, y, w, h) = Self::tooltip_geometry(atlas, &lines, at, screen, [0.0; 2]);
        self.tooltip_box(batch, None, x, y, w, h);
        Self::tooltip_text(batch, atlas, &lines, x, y);
    }
    /// `CreativeModeInventoryScreen.getTooltipFromContainerItem`: the lines
    /// that follow the item's name, none for a category tab's own grid slots
    /// (`grid`): the names of the tabs containing the stack, in blue, then on
    /// the search tab's grid the tags a `#` search shows, in dark purple.
    pub(super) fn creative_tooltip_tabs(
        &self,
        stack: &ItemStack,
        grid: bool,
    ) -> Vec<(horizon_client::text::Text, u32)> {
        let screen = &self.creative_screen;
        let tab = screen.tab();
        let category = !tab.search && !tab.inventory;
        if !horizon_client::creative::tooltip_lists_tabs(category, grid) {
            return Vec::new();
        }
        let blue = rgba(85, 85, 255, 255);
        let dark_purple = rgba(170, 0, 170, 255);
        let mut lines: Vec<(horizon_client::text::Text, u32)> = screen
            .tabs_containing(stack)
            .into_iter()
            .map(|i| {
                let title = self.creative_tab_title(&screen.tabs[i]);
                (horizon_client::text::Text::literal(title), blue)
            })
            .collect();
        if tab.search && grid {
            for tag in screen.search_tag_lines(stack) {
                lines.push((horizon_client::text::Text::literal(tag), dark_purple));
            }
        }
        lines
    }
    /// `CreativeModeInventoryScreen.extractRenderState`: item slots share the
    /// container screen's hover target for their highlight and tooltip.
    pub(super) fn draw_creative(&mut self, batch: &mut HudBatch) {
        // CreativeModeInventoryScreen: `CommonComponents.EMPTY` for a title.
        screen_info::begin_container("creative");
        let (ox, oy) = Self::creative_origin(batch);
        let selected = self.creative_screen.selected;
        // Only what is drawn of the tabs: their stacks stay where they are.
        let tabs = self.creative_screen.tab_headers();
        let Some(tab) = tabs.get(selected.min(tabs.len().saturating_sub(1))) else {
            return;
        };
        let tab_sprite = |t: &horizon_client::creative::Tab, on: bool| {
            format!(
                "gui/sprites/container/creative_inventory/tab_{}_{}_{}",
                if t.top { "top" } else { "bottom" },
                if on { "selected" } else { "unselected" },
                t.column + 1
            )
        };
        self.render_transparent_background("creative", batch);
        // Unselected tabs behind the panel, then the panel, then the selected tab.
        for (i, t) in tabs.iter().enumerate() {
            if i != selected && self.creative_screen.on_page(t) {
                let (tx, ty) = Creative::tab_position(t);
                batch.sprite(&self.gui.atlas, &tab_sprite(t, false), ox + tx, oy + ty);
                let mouse = (self.cursor.0 - ox, self.cursor.1 - oy);
                if Creative::tab_wants_hand(t, false, mouse) {
                    horizon_client::widgets::request_cursor(
                        horizon_client::widgets::Cursor::PointingHand,
                    );
                }
            }
        }
        // The scrollbar comes after the tabs, so its request wins where they meet.
        let scroll_mouse = (self.cursor.0 - ox, self.cursor.1 - oy);
        if let Some(cursor) = self
            .creative_screen
            .scrollbar_cursor(scroll_mouse, &self.pack.names)
        {
            horizon_client::widgets::request_cursor(cursor);
        }
        let background = if tab.search {
            "gui/container/creative_inventory/tab_item_search"
        } else if tab.inventory {
            "gui/container/creative_inventory/tab_inventory"
        } else {
            "gui/container/creative_inventory/tab_items"
        };
        if let Some(mut r) = self.gui.atlas.sprites.get(background).copied() {
            r.u1 = r.u0 + (r.u1 - r.u0) * Creative::WIDTH / r.w as f32;
            r.v1 = r.v0 + (r.v1 - r.v0) * Creative::HEIGHT / r.h as f32;
            batch.rect_uv(ox, oy, Creative::WIDTH, Creative::HEIGHT, r, u32::MAX);
        }
        let (tx, ty) = Creative::tab_position(tab);
        batch.sprite(&self.gui.atlas, &tab_sprite(tab, true), ox + tx, oy + ty);
        self.draw_screen_phase(batch, horizon_client::screen_draw::RenderPhase::Background);
        for t in tabs.iter().filter(|t| self.creative_screen.on_page(t)) {
            let (tx, ty) = Creative::tab_position(t);
            self.draw_item(
                batch,
                &t.icon,
                ox + tx + 5.0,
                oy + ty + if t.top { 9.0 } else { 7.0 },
            );
        }
        let atlas = &self.gui.atlas;
        if !tab.inventory {
            let title = self.creative_tab_title(tab);
            batch.text(
                atlas,
                &title,
                ox + 8.0,
                oy + 6.0,
                tab.name
                    .as_ref()
                    .and_then(|n| n.1)
                    .map_or(rgba(64, 64, 64, 255), |c| {
                        rgba((c >> 16) as u8, (c >> 8) as u8, c as u8, 255)
                    }),
                false,
            );
        }
        if tab.search {
            // CreativeModeInventoryScreen: an unbordered EditBox of 50 at (82, 6).
            let search = &mut self.creative_screen.search;
            search.x = ox + 82.0;
            search.y = oy + 6.0;
            search.width = 80.0;
            search.bordered = false;
            search.focused = true;
            search.text_color = u32::MAX;
            search.max_length = clipboard::CREATIVE_SEARCH_CHARS;
            if horizon_client::widgets::narrating() {
                search.message = screen_info::tr("itemGroup.search", &[]);
            }
            search.draw(batch, atlas, None);
        }
        // The hovered stack, and whether it is a grid slot (`CustomCreativeSlot`).
        let mut hovered: Option<(ItemStack, bool)> = None;
        // Use the same padded slot bounds as clicks and the other menus, so
        // highlights and tooltips remain visible between adjacent items.
        let hovered_slot = self.slot_at(batch, self.cursor.0, self.cursor.1);
        if tab.inventory {
            self.draw_player_preview(
                batch,
                ox + 73.0,
                oy + 6.0,
                ox + 105.0,
                oy + 49.0,
                20.0,
                0.0625,
            );
            let trash = Self::creative_trash_hovered(batch, self.cursor);
            let (tx, ty) = Creative::TRASH_POSITION;
            if trash {
                self.draw_slot_highlight(batch, false, ox + tx, oy + ty);
            }
            self.draw_hovered_slot_highlight(batch, false, ox, oy);
            self.draw_inventory_slots(batch, ox, oy);
            if trash {
                self.draw_slot_highlight(batch, true, ox + tx, oy + ty);
            }
            self.draw_hovered_slot_highlight(batch, true, ox, oy);
        } else {
            let items = self.creative_screen.items(&self.pack.names);
            let first = self.creative_screen.first_row(&self.pack.names);
            for r in 0..5 {
                for c in 0..9 {
                    let Some(stack) = items.get((first + r) * 9 + c).filter(|s| s.count > 0) else {
                        continue;
                    };
                    let (x, y) = (ox + 9.0 + c as f32 * 18.0, oy + 18.0 + r as f32 * 18.0);
                    let over = hovered_slot == Some(Creative::DISPLAY + r * 9 + c);
                    if over {
                        self.draw_slot_highlight(batch, false, x, y);
                    }
                    self.draw_item(batch, stack, x, y);
                    if over {
                        self.draw_slot_highlight(batch, true, x, y);
                        hovered = Some((stack.clone(), true));
                    }
                }
            }
            let drag = self.drag_view();
            for c in 0..9 {
                let (x, y) = (ox + 9.0 + c as f32 * 18.0, oy + 112.0);
                let over = hovered_slot == Some(c);
                if over {
                    self.draw_slot_highlight(batch, false, x, y);
                }
                self.draw_slot_item(batch, drag.as_ref(), c, &self.inventory[c], x, y);
                if over {
                    self.draw_slot_highlight(batch, true, x, y);
                    hovered = self.inventory[c].clone().map(|s| (s, false));
                }
            }
            let scrollable = self.creative_screen.rows(&self.pack.names) > 5;
            let scroller = if scrollable {
                "gui/sprites/container/creative_inventory/scroller"
            } else {
                "gui/sprites/container/creative_inventory/scroller_disabled"
            };
            batch.sprite(
                &self.gui.atlas,
                scroller,
                ox + Creative::SCROLL_X,
                oy + Creative::SCROLL_Y + self.creative_screen.scroller_offset(),
            );
        }
        let effect_tip = self.draw_inventory_effects(batch, (ox, oy), Creative::WIDTH as i32);
        self.draw_creative_pages(batch, ox, oy);
        let browser_tip = self.draw_mod_browser(batch);
        self.draw_screen_phase(batch, horizon_client::screen_draw::RenderPhase::Foreground);
        self.draw_carried(batch);
        if tab.inventory {
            if Self::creative_trash_hovered(batch, self.cursor) {
                // `CreativeModeInventoryScreen`: the destroy slot's tooltip is
                // `inventory.binSlot`, not an English literal.
                let destroy = self.translate_with_fallback("inventory.binSlot", "Destroy Item");
                self.tooltip(batch, &destroy, self.cursor.0, self.cursor.1);
            } else if let Some(slot) = self.slot_at(batch, self.cursor.0, self.cursor.1)
                && let Some(stack) = self.slot_view(slot)
                && self.tooltip_with_carried(&stack)
            {
                let tabs = self.creative_tooltip_tabs(&stack, false);
                self.draw_stack_tooltip_with(
                    batch,
                    &stack,
                    false,
                    self.cursor.0,
                    self.cursor.1,
                    tabs,
                );
            }
        } else if let Some((item, grid)) = hovered
            && self.tooltip_with_carried(&item)
        {
            let tabs = self.creative_tooltip_tabs(&item, grid);
            // A `CustomCreativeSlot`: TooltipFlag.asCreative.
            self.draw_stack_tooltip_with(batch, &item, grid, self.cursor.0, self.cursor.1, tabs);
        }
        if let Some(lines) = effect_tip {
            self.draw_tooltip_lines(batch, &lines, self.cursor.0, self.cursor.1);
        }
        // CreativeModeInventoryScreen.checkTabHovering: a tab says its name.
        if let Some(i) =
            Creative::hovered_tab(&tabs, self.creative_screen.page, (ox, oy), self.cursor)
            && let Some(t) = tabs.get(i)
        {
            let title = self.creative_tab_title(t);
            self.tooltip(batch, &title, self.cursor.0, self.cursor.1);
        }
        if let Some(tooltip) = browser_tip {
            self.draw_mod_browser_tooltip(batch, tooltip);
        }
    }
    pub(super) fn creative_trash_hovered(batch: &HudBatch, cursor: (f32, f32)) -> bool {
        let (ox, oy) = Self::creative_origin(batch);
        let (x, y) = Creative::TRASH_POSITION;
        cursor.0 >= ox + x
            && cursor.0 < ox + x + 16.0
            && cursor.1 >= oy + y
            && cursor.1 < oy + y + 16.0
    }
    /// NeoForge's creative page buttons, 20 square, 50 above the screen at
    /// its left and right edges: -1 or 1 for the one at `cursor`, when there
    /// is more than one page.
    pub(super) fn creative_page_button(
        ox: f32,
        oy: f32,
        cursor: (f32, f32),
        pages: u32,
    ) -> Option<i64> {
        if pages <= 1 {
            return None;
        }
        let y = oy - 50.0;
        let inside =
            |x: f32| cursor.0 >= x && cursor.0 < x + 20.0 && cursor.1 >= y && cursor.1 < y + 20.0;
        if inside(ox) {
            Some(-1)
        } else if inside(ox + Creative::WIDTH - 20.0) {
            Some(1)
        } else {
            None
        }
    }
    /// The page buttons and "page / pages" between them.
    pub(super) fn draw_creative_pages(&self, batch: &mut HudBatch, ox: f32, oy: f32) {
        let pages = self.creative_screen.pages();
        if pages <= 1 {
            return;
        }
        let atlas = &self.gui.atlas;
        let y = oy - 50.0;
        for (x, label, step) in [(ox, "<", -1), (ox + Creative::WIDTH - 20.0, ">", 1)] {
            let hover = Self::creative_page_button(ox, oy, self.cursor, pages) == Some(step);
            let page = self.creative_screen.page as i64 + step;
            let active = (0..pages as i64).contains(&page);
            let sprite = match (active, hover) {
                (false, _) => "gui/sprites/widget/button_disabled",
                (true, true) => "gui/sprites/widget/button_highlighted",
                (true, false) => "gui/sprites/widget/button",
            };
            horizon_client::ui::blit_sprite(batch, atlas, sprite, x, y, 20.0, 20.0, u32::MAX);
            let color = if active {
                u32::MAX
            } else {
                rgba(160, 160, 160, 255)
            };
            let w = HudBatch::text_width(atlas, label);
            batch.text(atlas, label, x + (20.0 - w) / 2.0, y + 6.0, color, true);
        }
        let text = format!("{} / {}", self.creative_screen.page + 1, pages);
        let w = HudBatch::text_width(atlas, &text);
        batch.text(
            atlas,
            &text,
            ox + (Creative::WIDTH - w) / 2.0,
            oy - 44.0,
            u32::MAX,
            true,
        );
    }
    pub(super) fn click_creative(&mut self, right: bool) {
        if self.mod_browser_press(right) {
            return;
        }
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return;
        };
        let batch = HudBatch::new(size, self.settings.gui_scale_inputs());
        let (ox, oy) = Self::creative_origin(&batch);
        let (cx, cy) = self.cursor;
        // NeoForge's page buttons, when mods add tabs.
        if let Some(step) =
            Self::creative_page_button(ox, oy, (cx, cy), self.creative_screen.pages())
        {
            // Page buttons are outside the inventory panel; their release must
            // not throw a carried stack, including when the button is disabled.
            self.inv_clicks.skip_release();
            let page = self.creative_screen.page as i64 + step;
            if (0..self.creative_screen.pages() as i64).contains(&page) {
                self.audio.play_button_click();
                self.creative_screen.set_page(page as u32);
                self.refresh_saved_hotbars();
            }
            return;
        }
        // Only the clicked tab's index is needed: cloning the tabs here copied
        // every tab's items on each click, saved hotbars' stacks included.
        let creative = &self.creative_screen;
        let clicked = creative.tabs.iter().position(|t| {
            let (tx, ty) = Creative::tab_position(t);
            creative.on_page(t)
                && cx >= ox + tx
                && cx < ox + tx + 26.0
                && cy >= oy + ty
                && cy < oy + ty + 32.0
        });
        if let Some(i) = clicked {
            select_creative_tab(&mut self.creative_screen, &mut self.inv_clicks, i);
            self.build_screen();
            self.refresh_saved_hotbars();
            self.inv_drag = None;
            self.last_inv_click = None;
            return;
        }
        if self.creative_inventory_open() {
            if Self::creative_trash_hovered(&batch, self.cursor) {
                if self.has_shift_down() {
                    let before = self.player_equipment();
                    self.inventory.fill(None);
                    self.armor.fill(None);
                    self.offhand = None;
                    self.equipment = Default::default();
                    self.craft_grid[..4].fill(None);
                    self.player_equipment_changed(&before);
                    self.apply_equipment();
                } else {
                    self.carried = None;
                }
                self.inv_drag = None;
                self.inv_clicks = Default::default();
            } else if self.slot_at(&batch, cx, cy).is_some() || self.clicked_outside(&batch) {
                self.inventory_press(right);
            }
            return;
        }
        if !right && self.creative_search_click(ox, oy) {
            return;
        }
        // Holding the scrollbar scrolls as the mouse moves (mouseDragged).
        if !right
            && self
                .creative_screen
                .press_scrollbar(cx - ox, cy - oy, &self.pack.names)
        {
            return;
        }
        if self.slot_at(&batch, cx, cy).is_some() || self.clicked_outside(&batch) {
            self.inventory_press(right);
        }
    }
    /// A click on the search tab's box (`EditBox.onClick`).
    pub(super) fn creative_search_click(&mut self, ox: f32, oy: f32) -> bool {
        if !self.creative_screen.tab().search {
            return false;
        }
        let (x, y, w, h) = (ox + 82.0, oy + 6.0, 80.0, 9.0);
        let (mx, my) = self.cursor;
        if mx < x || my < y || mx >= x + w || my >= y + h {
            return false;
        }
        let shift = self.has_shift_down();
        let search = &mut self.creative_screen.search;
        search.x = x;
        search.y = y;
        search.width = w;
        search.click(&self.gui.atlas, mx, shift, false);
        true
    }
}

//! Stat filter for the Item Info screen - the code half of Item Scroller.
//!
//! The rest of this mod is a `.ui` layout override (`ui/layout/item_info.ui`);
//! this DLL adds the filtering the layout format cannot express.
//!
//! # Why the control is hand-built
//!
//! The `.ui` grammar has a `dropdown` runner, but no layout anywhere declares
//! its entries - the exe fills them and owns the selection - and the stable API
//! exposes dropdown state as read-only (`state_get_json` returns
//! `{"selected_item"}`; `state_set_json` accepts only checkbox / text_edit /
//! slider / selectable). A spawned `dropdown` would render empty and stay
//! empty, so the control is a `color_selectable` header over a panel of
//! `color_selectable` rows, styled with the game's own `main#strategy_option`.
//!
//! That style paints nothing until hovered (`image` is `#00000000` on both fill
//! and stroke), so at rest the header read as bare text rather than a control.
//! It therefore overrides its resting `image` with `main#dropdown`'s frame and
//! carries the game's own chevron sprite, flipped to `dropdown_up` while the
//! panel is open. The caret is an `image` child with `ignore_event: true` - the
//! idiom `database_edit_component/number_list_row` uses for the icon inside its
//! own selectable - so it cannot swallow clicks meant for the header beneath.
//!
//! # Graying rather than hiding
//!
//! Non-matching slots are disabled, not hidden: the grid is a `child_type:
//! Table`, so hiding a child risks holes or a reflow we do not control, and a
//! grayed grid keeps every item where the eye last saw it. The slot template
//! (`item_info_component/item_slot`) is a `color_icon_button` on
//! `main#tertiary_button`, whose `disabled:` palette is muted at 65% alpha - so
//! `disable: true` is the game's own gray-out. Its `#icon:image` child is a
//! separate node the button palette does not reach, so that is tinted directly
//! (it has no explicit color in the template, meaning plain white is the
//! correct value to restore).
//!
//! # The class filter
//!
//! The second dropdown groups items the way `riot_items_tfm2` does - Assassin,
//! Fighter, Tank, Mage, Marksman, Support - and that mapping is data nowhere:
//! it is a table compiled into that pack's `item_catalog.rs`, and neither what
//! the pack ships (`config-default.json` is the player's balance file, stats
//! only) nor any client API reaches it. So the table is copied here, generated
//! from the pack's rather than retyped, and it has to be re-copied when the
//! pack adds items. An item the copy does not know simply has no class, which
//! reads as "grey under every class" rather than as a wrong class.
//!
//! Unlike the stat filter, an unknown item is greyed rather than left lit: the
//! base game's items and the pack's own components have no class at all, so
//! leaving every unclassified item lit would filter nothing.
//!
//! The menu hides itself when the pack is not installed. Every slug in the
//! table is one of the pack's own items, so a single match in the grid is
//! proof. The six base items it reskins as its `radiant_` tier are the
//! exception: they are classified, but they cannot count as proof, because
//! they sit in the grid with or without the pack.
//!
//! # Where item stats come from
//!
//! Two sources, because there is no single one:
//!
//! - `setting_get_json(ItemSetting, "")` - the game's item document. Covers the
//!   30 base items.
//! - `config-default.json` shipped by mods that register items in code. Items
//!   added through `StableMod::add_item` live in an item vtable that no client
//!   API exposes, so their stats are unreachable at runtime; the Riot pack
//!   ships its 130 items' stats as data next to its DLL, keyed by the same ids
//!   and the same stat names. Without this, ~80% of a modded grid is unreadable
//!   and stays un-grayed.
//!
//! Items resolved by neither source are left lit rather than grayed, so an
//! unknown item is never wrongly dimmed.

use mod_api_stable::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

const MOD_ID: &str = "item_scroller_tfm2";

/// The exe addresses the grid as `data.item_list.contents`, but the screen's
/// root prefix is not part of the stable contract, so we search for the node.
const LIST_NODE: &str = "item_list";
const CONTENTS_NODE: &str = "contents";
/// The item art inside a slot, from the `item_slot` template.
const SLOT_ICON: &str = "icon";

const ROOT_NODE: &str = "item_filter";
/// The card column, and the node the control keeps itself pinned above.
const CARD_NODE: &str = "item_detail_bg";
const CARET_NODE: &str = "caret";

const MAX_DEPTH: usize = 16;
const SEARCH_INTERVAL_FRAMES: u32 = 30;

/// Steam app id, for locating subscribed mods alongside the game folder.
const APP_ID: &str = "3009300";
/// Convention filename for a code mod that ships its item stats as data.
const MOD_ITEM_CONFIG: &str = "config-default.json";

/// The game's own dropdown chevron and its flipped twin. Both sprites are
/// 8.78x5.06, so swapping one for the other never moves or resizes the caret.
const CARET_DOWN: &str = "source: \"asset/base/ui/icons/dropdown\";";
const CARET_UP: &str = "source: \"asset/base/ui/icons/dropdown_up\";";

const DIM_SLOT: &str = "disable: true;";
const LIT_SLOT: &str = "disable: false;";
const DIM_ICON: &str = "color: #ffffff59;";
const LIT_ICON: &str = "color: #ffffffff;";

/// Written by the click handlers, which get only a reduced context and cannot
/// touch our state; one slot per menu. `usize::MAX` means "nothing clicked
/// since last read".
static CLICKED_ROW: [AtomicUsize; MENU_COUNT] =
    [const { AtomicUsize::new(usize::MAX) }; MENU_COUNT];
static CLICKED_HEAD: [AtomicBool; MENU_COUNT] = [const { AtomicBool::new(false) }; MENU_COUNT];

/// The dropdown, in order. Index 0 clears the filter; the rest match an item if
/// it grants ANY of the listed stat keys, so the flat and percentage forms both
/// count (an item giving +10% Attack Damage does grant AD). `adaptive_force`
/// scales with whichever of AD/AP the holder favours, so it counts for both.
const FILTERS: [(&str, &[&str]); 12] = [
    ("All Items", &[]),
    (
        "Attack Damage",
        &["attack", "attack_mult", "adaptive_force"],
    ),
    (
        "Magic Power",
        &["magic_power", "magic_power_mult", "adaptive_force"],
    ),
    ("Attack Speed", &["attack_speed_mult"]),
    ("Cooldown Reduction", &["skill_cooldown_mult"]),
    ("Crit Chance", &["crit_chance"]),
    ("Health", &["hp", "hp_mult"]),
    ("Armor", &["defence", "defence_mult"]),
    (
        "Magic Resist",
        &["magic_resistance", "magic_resistance_mult"],
    ),
    ("Omnivamp", &["vamp"]),
    ("Movement Speed", &["move_speed_mult"]),
    ("Tenacity", &["toughness"]),
];

/// The class dropdown, in order. Index 0 clears the filter; every later index
/// is a class code in [`CLASS_OF`], offset by one.
const CLASSES: [&str; 7] = [
    "All Classes",
    "Assassin",
    "Fighter",
    "Tank",
    "Mage",
    "Marksman",
    "Support",
];

/// `riot_items_tfm2`'s item -> class table, generated from the `CATEGORY_OF`
/// compiled into that pack's `item_catalog.rs`. Sorted by slug, for
/// `binary_search_by_key`. Codes index [`CLASSES`] minus its first entry.
const CLASS_OF: [(&str, u8); 74] = [
    ("ardent_censer", 5),
    ("atmas_reckoning", 2),
    ("axiom_arc", 0),
    ("bastionbreaker", 0),
    ("black_cleaver", 1),
    ("blackfire_torch", 3),
    ("blade_of_the_ruined_king", 4),
    ("bloodletters_curse", 3),
    ("bloodsong", 5),
    ("bloodthirster", 1),
    ("cloak_of_starry_night", 2),
    ("collector", 0),
    ("dead_mans_plate", 2),
    ("deathblade", 4),
    ("deaths_dance", 1),
    ("diamond_tipped_spear", 4),
    ("dragons_claw", 2),
    ("dusk_and_dawn", 3),
    ("echoes_of_helia", 5),
    ("eclipse", 1),
    ("experimental_hexplate", 1),
    ("feral_flare", 1),
    ("frozen_heart", 2),
    ("frozen_mallet", 1),
    ("grezs_spectral_lantern", 3),
    ("guinsoos_rageblade", 4),
    ("hamstringer", 4),
    ("heartsteel", 2),
    ("hextech_gunblade", 3),
    ("hubris", 0),
    ("infinity_edge", 4),
    ("jaksho_the_protean", 2),
    ("kraken_slayer", 4),
    ("liandrys_torment", 3),
    ("locket_of_the_iron_solari", 5),
    ("lord_dominiks_regards", 4),
    ("ludens_tempest", 3),
    ("malignance", 3),
    ("mirage_blade", 4),
    ("morellonomicon", 3),
    ("mortal_reminder", 4),
    ("nashors_tooth", 3),
    ("night_harvester", 3),
    ("opportunity", 0),
    ("overlords_bloodmail", 1),
    ("phantom_dancer", 4),
    ("protectors_vow", 2),
    ("protoplasm_harness", 5),
    ("rabadons_deathcap", 3),
    ("randuins_omen", 2),
    ("ravenous_hydra", 1),
    ("riftmaker", 3),
    ("rite_of_ruin", 3),
    ("rylais_crystal_scepter", 3),
    ("serpents_fang", 0),
    ("shadowflame", 3),
    ("spear_of_shojin", 1),
    ("spirit_visage", 2),
    ("steraks_gage", 1),
    ("stormrazor", 4),
    ("stormsurge", 3),
    ("sundered_sky", 1),
    ("sunfire_cape", 2),
    ("sword_of_blossoming_dawn", 5),
    ("terminus", 4),
    ("thornmail", 2),
    ("trinity_force", 1),
    ("unending_despair", 2),
    ("void_staff", 3),
    ("voltaic_cyclosword", 0),
    ("warmogs_armor", 2),
    ("wits_end", 4),
    ("yun_tal_wildarrows", 4),
    ("zekes_herald", 5),
];

/// The six base game items the pack reskins as its `radiant_` tier, mapped to
/// the slug the class table knows them by. They keep their base ids, so they
/// are the one kind of classified item that is in the grid whether or not the
/// pack is installed - which is why [`pack_class`] does not look at them.
const RESKINNED: [(&str, &str); 6] = [
    ("giants_horn_shard", "sunfire_cape"),
    ("impregnable_fortress", "thornmail"),
    ("prophet_of_the_abyss", "ludens_tempest"),
    ("storm_sovereign", "phantom_dancer"),
    ("veil_of_annihilation", "dragons_claw"),
    ("warlords_final_judgement", "bloodthirster"),
];

fn class_of_slug(slug: &str) -> Option<u8> {
    CLASS_OF
        .binary_search_by_key(&slug, |(key, _)| *key)
        .ok()
        .map(|index| CLASS_OF[index].1)
}

/// The class of an item the pack itself adds - and so also the test for whether
/// the pack is installed at all, since every slug in the table is one of its
/// items. A `radiant_` upgrade carries its base item's id under the prefix.
fn pack_class(id: &str) -> Option<u8> {
    class_of_slug(id.strip_prefix("radiant_").unwrap_or(id))
}

/// The class of any item in the grid, the reskinned base items included.
fn class_of(id: &str) -> Option<u8> {
    pack_class(id).or_else(|| {
        RESKINNED
            .iter()
            .find(|(key, _)| *key == id)
            .and_then(|(_, slug)| class_of_slug(slug))
    })
}

/// The class code a dropdown index selects, or `None` for "All Classes".
fn class_filter(index: usize) -> Option<u8> {
    index.checked_sub(1).map(|code| code as u8)
}

// --- paths ----------------------------------------------------------------

fn join(path: &str, child: &str) -> String {
    if path.is_empty() {
        child.to_string()
    } else {
        format!("{path}.{child}")
    }
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('.').map_or("", |(head, _)| head)
}

fn find_node(ctx: &StableClient<'_>, path: &str, target: &str, depth: usize) -> Option<String> {
    if depth > MAX_DEPTH {
        return None;
    }
    ctx.ui_child_names(path).into_iter().find_map(|child| {
        let child_path = join(path, &child);
        if child == target {
            Some(child_path)
        } else {
            find_node(ctx, &child_path, target, depth + 1)
        }
    })
}

// --- item data ------------------------------------------------------------

/// item id -> the stat keys it grants (only non-zero ones are kept).
type ItemStats = BTreeMap<String, Vec<String>>;

fn nonzero(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(flag) => *flag,
        serde_json::Value::Number(number) => number.as_f64().is_some_and(|value| value != 0.0),
        _ => false,
    }
}

fn record(items: &mut ItemStats, id: &str, keys: impl Iterator<Item = String>) {
    let entry = items.entry(id.to_string()).or_default();
    for key in keys {
        if !entry.contains(&key) {
            entry.push(key);
        }
    }
}

/// The game's item document: `{ "<id>": { "key": "<id>", "stat": { ... } },
/// "mod_items": [...] }`.
///
/// The map key and the entry's own `key` agree for all but one base item -
/// `iron_blade` is `ironsword` - and it is the inner `key` the grid names its
/// slots by, so that is what we file under.
fn absorb_item_setting(document: &str, items: &mut ItemStats) {
    let Ok(root) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(document)
    else {
        return;
    };

    let granted = |entry: &serde_json::Value| -> Vec<String> {
        entry
            .get("stat")
            .and_then(|stat| stat.as_object())
            .map(|stat| {
                stat.iter()
                    .filter(|(_, value)| nonzero(value))
                    .map(|(key, _)| key.clone())
                    .collect()
            })
            .unwrap_or_default()
    };

    for (id, entry) in &root {
        if id == "mod_items" {
            for entry in entry.as_array().into_iter().flatten() {
                // Mod items carry their own id; the field name is not in the
                // stable contract, so accept the usual spellings.
                let id = ["id", "key", "name"]
                    .iter()
                    .find_map(|field| entry.get(field).and_then(|value| value.as_str()));
                if let Some(id) = id {
                    record(items, id, granted(entry).into_iter());
                }
            }
        } else {
            let id = entry
                .get("key")
                .and_then(|value| value.as_str())
                .filter(|key| !key.is_empty())
                .unwrap_or(id);
            record(items, id, granted(entry).into_iter());
        }
    }
}

/// A code mod's shipped stat table: `{ "<id>": { "attack": 65, ... } }` - flat,
/// with the same stat names the game uses. `price` and the `effect_*` keys
/// describe passives rather than granted stats, so they are skipped.
fn absorb_mod_config(document: &str, items: &mut ItemStats) {
    let document = document.trim_start_matches('\u{feff}');
    let Ok(root) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(document)
    else {
        return;
    };

    for (id, entry) in &root {
        let Some(fields) = entry.as_object() else {
            continue;
        };
        let keys = fields
            .iter()
            .filter(|(key, value)| {
                key.as_str() != "price" && !key.starts_with("effect_") && nonzero(value)
            })
            .map(|(key, _)| key.clone());
        record(items, id, keys);
    }
}

/// Mod folders to look in: the game's own `mods/`, and subscribed Workshop
/// items, which live beside the game install rather than inside it.
fn mod_roots() -> Vec<PathBuf> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    let mut roots = vec![cwd.join("mods")];
    // ...steamapps/common/<game> -> ...steamapps/workshop/content/<app id>
    if let Some(steamapps) = cwd.parent().and_then(|common| common.parent()) {
        roots.push(steamapps.join("workshop").join("content").join(APP_ID));
    }
    roots
}

fn absorb_mod_configs(items: &mut ItemStats) {
    for root in mod_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let config = entry.path().join(MOD_ITEM_CONFIG);
            if let Ok(text) = std::fs::read_to_string(&config) {
                absorb_mod_config(&text, items);
            }
        }
    }
}

// --- the control ----------------------------------------------------------

const ROW_WIDTH: u32 = 246;
const ROW_HEIGHT: u32 = 28;
const PANEL_WIDTH: u32 = 260;
const HEAD_WIDTH: u32 = 260;
const HEAD_HEIGHT: u32 = 32;
/// Space between the two heads. The pair is right-anchored, so it grows left
/// into the gap above the item grid: 2 * 260 + 8 leaves 12px of the 540px root
/// to spare, and stops 64px clear of `#sub_tabs`, which ends at x 1008.
const MENU_GAP: u32 = 8;
/// The pair is only ever as wide as the root it is anchored inside.
const _: () = assert!(HEAD_WIDTH * 2 + MENU_GAP <= ROOT_WIDTH);
/// Right inset of the caret, matching `main#dropdown`'s own `icon_layout`.
const CARET_INSET: u32 = 20;
/// Gap between rows, and the panel's inset. Both are written into the spawn
/// source below, so they live here to keep `panel_height` honest.
const ROW_SPACING: u32 = 2;
const PANEL_PADDING: u32 = 6;
/// Where the panel hangs below the head, and how tall the root is with the
/// panel closed.
const PANEL_Y: u32 = 36;
const ROOT_HEIGHT: u32 = 40;
/// The root spans the card column, so the right-anchored head lands over it.
const ROOT_WIDTH: u32 = 540;
const ROOT_X: i32 = 1060;
/// Where `#item_detail_bg` sits on the full Game Info screen: below the tab
/// strip. The prematch popup has no tab strip and the exe shifts it up to 0.
const CARD_Y: u32 = 52;
/// Space left between the control's bottom edge and the top of the card column.
const CONTROL_GAP: u32 = 6;
/// How far left the control is pulled once it has ridden up out of the screen
/// body and into the prematch popup's title bar, whose right end is the close
/// button. Without it the head would sit under the X.
const POPUP_X_INSET: i32 = 50;
/// How much further up the control goes in that same title bar. `place` can
/// only measure down to the top of the card, and the bar is chrome the exe
/// draws outside `#data`, so there is no node to centre against - this is the
/// rest of the way to level with the title and the close button.
const POPUP_Y_LIFT: i32 = 6;

const MENU_COUNT: usize = 2;
/// Index of each menu in [`MENUS`], and into `State::current`.
const STAT: usize = 0;
const CLASS: usize = 1;

#[derive(Clone, Copy)]
struct Menu {
    head: &'static str,
    panel: &'static str,
    /// Offset from the root's right edge. Both menus are right-anchored, so
    /// the stat one keeps the spot it has always had and the class one hangs
    /// off its left.
    x: i32,
}

const MENUS: [Menu; MENU_COUNT] = [
    Menu {
        head: "head",
        panel: "panel",
        x: 0,
    },
    Menu {
        head: "class_head",
        panel: "class_panel",
        x: -((HEAD_WIDTH + MENU_GAP) as i32),
    },
];

fn options(menu: usize) -> usize {
    if menu == STAT {
        FILTERS.len()
    } else {
        CLASSES.len()
    }
}

fn option_label(menu: usize, index: usize) -> &'static str {
    if menu == STAT {
        FILTERS[index].0
    } else {
        CLASSES[index]
    }
}

/// A head shows the chosen option, and does so twice: the runner reads the
/// declared `text` property, and `ui_set_text` covers the label node itself.
fn set_head_label(ctx: &mut StableClient<'_>, head: &str, label: &str) {
    ctx.ui_set_properties(head, &format!("text: \"{label}\";"));
    ctx.ui_set_text(head, label);
}

/// The panel paints its own background, and a painted node needs a real height.
///
/// With `height: auto` the fill covered only the first row while the rest of the
/// list drew straight over the item card behind it. Every other `auto` in this
/// mod is on an `:empty` or `:label` node, and the panel's parent
/// (`item_filter`) is a fixed 40px, so `auto` has nothing to grow against.
/// Computed rather than hardcoded so adding a filter cannot silently re-open the
/// same gap.
fn panel_height(menu: usize) -> u32 {
    let rows = options(menu) as u32;
    rows * ROW_HEIGHT + rows.saturating_sub(1) * ROW_SPACING + PANEL_PADDING * 2
}

/// How tall the root has to be for the open panel to fit inside it. The two
/// panels are different lengths, so this is asked of whichever one is open.
fn root_open_height(menu: usize) -> u32 {
    PANEL_Y + panel_height(menu)
}

/// Shows the open menu's panel and hides the other's - only ever one at a
/// time, so the two lists can never overlap.
///
/// The panel is a child of the root, and the root is a fixed 40px strip, so the
/// panel's painted background was being clipped to that 40px — covering only the
/// first row while the `color_selectable` rows, which draw separately and carry
/// no `z`, spilled out over the item card behind. Giving the panel its own
/// height was not enough on its own; the root has to make room for it. It is
/// restored on close so the collapsed control still occupies just its own strip
/// and never sits over the card.
fn set_open(ctx: &mut StableClient<'_>, root: &str, open: Option<usize>) {
    for (menu, spec) in MENUS.iter().enumerate() {
        let shown = open == Some(menu);
        ctx.ui_set_visible(&join(root, spec.panel), shown);
        let caret = join(&join(root, spec.head), CARET_NODE);
        ctx.ui_set_properties(&caret, if shown { CARET_UP } else { CARET_DOWN });
    }
    let height = open.map_or(ROOT_HEIGHT, root_open_height);
    ctx.ui_set_properties(root, &format!("height: {height}px;"));
}

/// One menu: the head, and the panel that drops out of it.
fn menu_source(menu: usize) -> String {
    let mut rows = String::new();
    for index in 0..options(menu) {
        let label = option_label(menu, index);
        rows.push_str(&format!(
            "#opt{index}:color_selectable {{ @\"asset/base/style/main#strategy_option\"; \
             width: {ROW_WIDTH}px; height: {ROW_HEIGHT}px; \
             label: {{ size: 14; }} selected_label: {{ size: 14; }} \
             text: \"{label}\"; }} "
        ));
    }

    let Menu { head, panel, x } = MENUS[menu];
    let height = panel_height(menu);
    let title = option_label(menu, 0);
    // The class menu stays hidden until the pack whose classes it lists has
    // been seen in the grid, so it never offers a filter that matches nothing.
    let hidden = if menu == STAT { "" } else { "visible: false; " };
    format!(
        "#{head}:color_selectable {{ @\"asset/base/style/main#strategy_option\"; \
           anchor_x: 1; pivot_x: 1; x: {x}px; {hidden}\
           width: {HEAD_WIDTH}px; height: {HEAD_HEIGHT}px; \
           image: {{ color: #4a4c56ff; back_color: #1d1f2cff; stroke: 1; \
                     rounding: Uniform {{ rounding: 8; }} \
                     hover: {{ color: #a5a5abff; }} }} \
           label: {{ size: 15; }} selected_label: {{ size: 15; }} \
           text: \"{title}\"; \
           #{CARET_NODE}:image {{ {CARET_DOWN} ignore_event: true; color: #a5a5abff; \
             anchor_x: 1; pivot_x: 1; x: -{CARET_INSET}px; \
             anchor_y: 0.5; pivot_y: 0.5; width: 8.78px; height: 5.06px; }} }} \
         #{panel}:color {{ anchor_x: 1; pivot_x: 1; x: {x}px; y: {PANEL_Y}px; \
           width: {PANEL_WIDTH}px; height: {height}px; visible: false; \
           color: #4a4c56ff; back_color: #161721ff; stroke: 1; \
           rounding: Uniform {{ rounding: 8; }} \
           padding: {{ left: {PANEL_PADDING}px; right: {PANEL_PADDING}px; \
                       top: {PANEL_PADDING}px; bottom: {PANEL_PADDING}px; }} \
           child_type: TopToBottom {{ spacing: {ROW_SPACING}px; }} \
           {rows} }} "
    )
}

fn control_source() -> String {
    let menus: String = (0..MENU_COUNT).map(menu_source).collect();
    // Where the control lands on the full Game Info screen; `place` corrects it
    // from the card's real position as soon as there is a layout pass to read.
    let spawn_y = CARD_Y as i32 - (ROOT_HEIGHT + CONTROL_GAP) as i32;
    format!(
        "{ROOT_NODE}:empty {{ x: {ROOT_X}px; y: {spawn_y}px; \
         width: {ROOT_WIDTH}px; height: {ROOT_HEIGHT}px; {menus} }}"
    )
}

/// Keeps the control pinned just above the card column, and reports the `y` it
/// settled on.
///
/// `#item_detail_bg` is declared at `y: 52`, clear of the tab strip above it.
/// The prematch popup has no tab strip, so the exe shifts the whole column up
/// to `y: 0` - and the control, being our node rather than one of the exe's,
/// did not come along. It stayed at `y: 6` and sat on the card's Tier label.
/// Measuring off the card each frame tracks whichever screen we are on instead
/// of guessing which one it is.
///
/// Rects come back in drawn pixels, so they are divided by the scale the root
/// is drawn at - its measured width over the `ROOT_WIDTH` it declares - to get
/// back to the units `ui_set_properties` expects.
fn place(ctx: &StableClient<'_>, host: &str, root: &str) -> Option<i32> {
    let (_, host_y, _, _) = ctx.ui_node_rect(host)?;
    let (_, card_y, _, _) = ctx.ui_node_rect(&join(host, CARD_NODE))?;
    let (_, _, root_w, _) = ctx.ui_node_rect(root)?;

    let scale = root_w / ROOT_WIDTH as f32;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let card_top = ((card_y - host_y) / scale).round() as i32;
    Some(card_top - (ROOT_HEIGHT + CONTROL_GAP) as i32)
}

// --- filtering ------------------------------------------------------------

/// Applies both filters by graying every slot that fails either one.
///
/// A slot we have no stats for is left lit, but a slot we have no class for is
/// grayed: see the class-filter note at the top of the file for why the two
/// unknowns are treated as opposites.
fn apply(
    items: &ItemStats,
    ctx: &mut StableClient<'_>,
    contents: &str,
    keys: &[&str],
    class: Option<u8>,
) {
    let mut unresolved: Vec<String> = Vec::new();

    for child in ctx.ui_child_names(contents) {
        let slot = join(contents, &child);
        let icon = join(&slot, SLOT_ICON);
        // The tier headers are children of the grid too, and are not items.
        // The slot template's `#icon` is what tells the two apart.
        if !ctx.ui_exists(&icon) {
            continue;
        }

        let stat_dim = !keys.is_empty()
            && match items.get(&child) {
                Some(granted) => !keys.iter().any(|key| granted.iter().any(|had| had == key)),
                None => {
                    unresolved.push(child.clone());
                    false
                }
            };
        let class_dim = class.is_some_and(|wanted| class_of(&child) != Some(wanted));
        let dim = stat_dim || class_dim;

        ctx.ui_set_properties(&slot, if dim { DIM_SLOT } else { LIT_SLOT });
        ctx.ui_set_properties(&icon, if dim { DIM_ICON } else { LIT_ICON });
    }

    dump_unresolved(&unresolved);
}

/// One-shot dump of the child names `apply` could not resolve to an item.
///
/// `apply` deliberately leaves them lit so a real item is never wrongly dimmed,
/// which also means a name that never resolves stays highlighted under every
/// filter with nothing on screen saying so. Written once, beside the game
/// executable, purely so they can be identified.
static UNRESOLVED_DUMPED: AtomicBool = AtomicBool::new(false);

fn dump_unresolved(names: &[String]) {
    if names.is_empty() || UNRESOLVED_DUMPED.swap(true, Ordering::Relaxed) {
        return;
    }
    let mut body = format!(
        "item_scroller_tfm2: {} child node name(s) in the item grid matched no entry in either stat source, so they stay lit under every filter:\n\n",
        names.len()
    );
    for name in names {
        body.push_str(name);
        body.push('\n');
    }
    if let Ok(cwd) = std::env::current_dir() {
        let _ = std::fs::write(cwd.join("item_scroller_unresolved.txt"), body);
    }
}

// --- extension ------------------------------------------------------------

#[derive(Default)]
struct State {
    /// Path of `item_list`, once found; cleared when the screen goes away.
    list: Option<String>,
    search_wait: u32,
    built: bool,
    /// Which menu's panel is down, if any. Only one is ever open.
    open: Option<usize>,
    /// Chosen option per menu, indexed by [`STAT`] / [`CLASS`].
    current: [usize; MENU_COUNT],
    items: ItemStats,
    loaded: bool,
    /// Whether the item pack that defines the classes is installed, which is
    /// also whether the class menu is on screen.
    classed: bool,
    applied: Option<([usize; MENU_COUNT], usize)>,
    /// Last `y` written by `place`, so the properties are only rewritten when
    /// the control actually has to move.
    placed: Option<i32>,
}

struct ItemFilter {
    state: Mutex<State>,
}

impl ItemFilter {
    /// Spawns the control and wires click handlers to the atomics.
    fn build(state: &mut State, ctx: &mut StableClient<'_>, host: &str) {
        if !ctx.ui_spawn_source(host, &control_source()) {
            return;
        }
        state.built = true;

        // A fresh control comes up on the labels its source declares - option 0
        // in both menus - so the filters have to come back to zero with it.
        // They used to outlive the control, and the grid kept a greying the
        // heads no longer admitted to: reopening Item Info from the item builds
        // screen showed "All Items" over a still-greyed grid. Writing the old
        // labels back onto the fresh heads instead does not work, because the
        // spawn is not addressable until a layout pass has run.
        state.current = [0; MENU_COUNT];
        state.open = None;

        let root = join(host, ROOT_NODE);
        for (menu, spec) in MENUS.iter().enumerate() {
            let head = join(&root, spec.head);
            ctx.ui_register_click(&head, "", move |_| {
                CLICKED_HEAD[menu].store(true, Ordering::Relaxed);
            });
            let panel = join(&root, spec.panel);
            for index in 0..options(menu) {
                ctx.ui_register_click(&join(&panel, &format!("opt{index}")), "", move |_| {
                    CLICKED_ROW[menu].store(index, Ordering::Relaxed);
                });
            }
        }

        // The clicks that chose the old filters may still be sitting in the
        // atomics if the screen went away between the click and this frame;
        // draining them here keeps them from selecting into the new control.
        for menu in 0..MENU_COUNT {
            CLICKED_HEAD[menu].store(false, Ordering::Relaxed);
            CLICKED_ROW[menu].store(usize::MAX, Ordering::Relaxed);
        }
    }

    /// Shows or hides the class menu, which only earns its place when the pack
    /// that defines the classes is installed. The grid is the test: every slug
    /// in [`CLASS_OF`] is one of the pack's own items.
    fn detect_classes(state: &mut State, ctx: &mut StableClient<'_>, root: &str, contents: &str) {
        let classed = ctx
            .ui_child_names(contents)
            .iter()
            .any(|name| pack_class(name).is_some());
        if classed == state.classed {
            return;
        }
        state.classed = classed;
        ctx.ui_set_visible(&join(root, MENUS[CLASS].head), classed);
        if !classed {
            // Nothing left to filter by, and no head to say so.
            state.current[CLASS] = 0;
            if state.open == Some(CLASS) {
                state.open = None;
            }
            set_open(ctx, root, state.open);
        }
    }

    /// Clicks arrive either through a registered handler or as the row's own
    /// `selected` flag; both are drained here, and the flag is cleared so the
    /// rows never look stuck.
    fn poll(state: &mut State, ctx: &mut StableClient<'_>, root: &str) -> bool {
        let mut changed = false;

        for (menu, spec) in MENUS.iter().enumerate() {
            // A hidden menu cannot be clicked, so there is nothing to drain.
            if menu == CLASS && !state.classed {
                continue;
            }

            let head = join(root, spec.head);
            let head_selected = ctx.ui_selectable_selected(&head).unwrap_or(false);
            if head_selected {
                ctx.ui_set_selectable_selected(&head, false);
            }
            if CLICKED_HEAD[menu].swap(false, Ordering::Relaxed) || head_selected {
                // Opening either menu closes the other, so the two panels can
                // never be down at once.
                state.open = (state.open != Some(menu)).then_some(menu);
                set_open(ctx, root, state.open);
            }

            let panel = join(root, spec.panel);
            let mut chosen = match CLICKED_ROW[menu].swap(usize::MAX, Ordering::Relaxed) {
                usize::MAX => None,
                index => Some(index),
            };
            for index in 0..options(menu) {
                let row = join(&panel, &format!("opt{index}"));
                if ctx.ui_selectable_selected(&row).unwrap_or(false) {
                    ctx.ui_set_selectable_selected(&row, false);
                    chosen = Some(index);
                }
            }

            let Some(index) = chosen.filter(|index| *index < options(menu)) else {
                continue;
            };

            state.current[menu] = index;
            state.open = None;
            set_open(ctx, root, None);
            set_head_label(ctx, &head, option_label(menu, index));
            changed = true;
        }

        changed
    }
}

impl StableExtension for ItemFilter {
    fn post_update(&self, ctx: &mut StableClient<'_>, _dt_micros: u64) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        // Drop a stale path the moment the screen closes, so the next open
        // re-finds it and re-spawns the control.
        if state
            .list
            .as_deref()
            .is_some_and(|path| !ctx.ui_exists(path))
        {
            state.list = None;
            state.built = false;
            state.open = None;
            state.classed = false;
            state.applied = None;
            state.placed = None;
        }

        if state.list.is_none() {
            match state.search_wait.checked_sub(1) {
                Some(remaining) => {
                    state.search_wait = remaining;
                    return;
                }
                None => {
                    state.search_wait = SEARCH_INTERVAL_FRAMES;
                    state.list = find_node(ctx, "", LIST_NODE, 0);
                }
            }
        }

        let Some(list) = state.list.clone() else {
            return;
        };
        let contents = join(&list, CONTENTS_NODE);
        if !ctx.ui_exists(&contents) {
            return;
        }
        let host = parent_of(&list).to_string();

        // Neither source can change while the game runs, so read them once.
        if !state.loaded {
            state.loaded = true;
            if let Some(document) = ctx.setting_get_json(SettingTargetV1::ItemSetting, "") {
                absorb_item_setting(&document, &mut state.items);
            }
            absorb_mod_configs(&mut state.items);
        }

        if !state.built {
            Self::build(&mut state, ctx, &host);
            if !state.built {
                return;
            }
        }

        let root = join(&host, ROOT_NODE);

        if let Some(y) = place(ctx, &host, &root) {
            if state.placed != Some(y) {
                state.placed = Some(y);
                // A negative `y` means the card had no strip above it to sit
                // in, so the control has ridden up into the prematch popup's
                // title bar: it has to clear the close button at that bar's
                // right end, and sit level with the title rather than hanging
                // below it.
                let (x, y) = if y < 0 {
                    (ROOT_X - POPUP_X_INSET, y - POPUP_Y_LIFT)
                } else {
                    (ROOT_X, y)
                };
                ctx.ui_set_properties(&root, &format!("x: {x}px; y: {y}px;"));
            }
        }

        // The game repopulates the grid when the tab is reopened, so a changed
        // child count is both when the filters have to be reapplied and when
        // the pack's items could first have appeared.
        let count = ctx.ui_child_count(&contents).unwrap_or(0);
        if state.applied.is_none_or(|(_, applied)| applied != count) {
            Self::detect_classes(&mut state, ctx, &root, &contents);
        }

        let changed = Self::poll(&mut state, ctx, &root);

        if changed || state.applied != Some((state.current, count)) {
            let keys = FILTERS[state.current[STAT]].1;
            let class = class_filter(state.current[CLASS]);
            apply(&state.items, ctx, &contents, keys, class);
            state.applied = Some((state.current, count));
        }
    }
}

fn init(host: &StableHost) -> StableMod {
    host.log(
        LogLevel::Info,
        "item_scroller_tfm2: item stat filter registering",
    );
    let mut reg = StableMod::new(MOD_ID);
    reg.set_extension(ItemFilter {
        state: Mutex::new(State::default()),
    });
    reg
}

declare_stable_mod!(init);

#[cfg(test)]
mod tests {
    use super::*;

    /// `class_of_slug` binary searches, and the table is a copy maintained by
    /// hand, so a re-copy that lands out of order has to fail loudly rather
    /// than silently mislaying items.
    #[test]
    fn class_table_is_sorted() {
        assert!(CLASS_OF.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(CLASS_OF
            .iter()
            .all(|(_, code)| (*code as usize) < CLASSES.len() - 1));
    }

    #[test]
    fn classes_resolve() {
        // The pack's own item, its radiant upgrade, and a base item the pack
        // reskins as a radiant - the last classified, but never proof the pack
        // is installed.
        assert_eq!(class_of("hubris"), Some(0));
        assert_eq!(class_of("radiant_thornmail"), Some(2));
        assert_eq!(class_of("impregnable_fortress"), Some(2));
        assert!(pack_class("impregnable_fortress").is_none());
        // A base item and one of the pack's components have no class at all.
        assert!(class_of("iron_blade").is_none());
        assert!(class_of("bf_sword").is_none());
    }
}

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
//! Fighter, Tank, Mage, Marksman, Support. The pack writes that mapping down in
//! one place, `text/item_classes.json` (`{ "<slug>": "<class>" }`), which its
//! build compiles in and which also ships with it. No client API reaches the
//! compiled copy, so the shipped file is read from wherever the pack is
//! installed (see [`pack_dirs`]), and new items in the pack are classed here
//! without a change to this mod. Its `Boots`
//! entries, and any class name not in [`CLASSES`], are skipped. An item the
//! file does not know simply has no class, which reads as "grey under every
//! class" rather than as a wrong class; with no file at all (the pack is not
//! installed, or predates it) nothing is classed and the menu stays hidden.
//!
//! Unlike the stat filter, an unknown item is greyed rather than left lit: the
//! base game's items and the pack's own components have no class at all, so
//! leaving every unclassified item lit would filter nothing.
//!
//! The menu hides itself when the pack is not in the grid - a subscribed but
//! disabled pack still has its file on disk. Every slug in the file is one of
//! the pack's own items, so a single match in the grid is proof. The six base items it reskins as its `radiant_` tier are the
//! exception: they are classified, but they cannot count as proof, because
//! they sit in the grid with or without the pack.
//!
//! # The Boots section
//!
//! The grid is built by the exe, one `tier_header` per item tier and the items
//! under their own tier, so `riot_items_tfm2`'s boots land in two places:
//! Boots under Starter, the seven upgrades under Epic. The grid cannot be told
//! otherwise, and the stable API cannot move a node, so [`regroup_boots`]
//! removes the exe's boots slots and appends a Boots header and rebuilt slots
//! after the last section. The rebuilt slots keep the exe's node names (the
//! item ids), which is what the filters above look items up by, and what gives
//! the exe's own click handling its best chance of still reaching them.
//!
//! It only runs when the pack itself is in the grid - the same test that shows
//! the class menu - so another mod's item that happens to share a boots id is
//! left where the exe put it.
//!
//! # Where item stats come from
//!
//! Three sources, because there is no single one:
//!
//! - `setting_get_json(ItemSetting, "")` - the game's item document. Covers the
//!   30 base items, but with their base stats: a mod's merge over that document
//!   does not show through it.
//! - `config-default.json` shipped by mods that register items in code. Items
//!   added through `StableMod::add_item` live in an item vtable that no client
//!   API exposes, so their stats are unreachable at runtime; the Riot pack
//!   ships its 130 items' stats as data next to its DLL, keyed by the same ids
//!   and the same stat names. Without this, ~80% of a modded grid is unreadable
//!   and stays un-grayed.
//! - `setting/item_setting.item_setting` in any mod whose `mod.override_info`
//!   merges it over the game's item document - how the Riot pack rebalances
//!   the base items it renames. Applied last, stat by stat, the way the game
//!   merges it, so it can take a stat away as well as add one.
//!
//! Items resolved by no source are left lit rather than grayed, so an unknown
//! item is never wrongly dimmed.
//!
//! # The total price
//!
//! The card's price is one upgrade step's: Bastionbreaker's 650 is what turns
//! a Serrated Dirk into it, not what it costs from nothing. [`show_total`]
//! replaces it with the whole way up - the item's price plus the cheapest and
//! the dearest way to the item it is built from, as one number or a range.
//! An item's `previous_tier` items are alternatives, any one of them, which is
//! how the Riot pack sums what a held item has cost in a match.
//!
//! The paths come from the same places as the stats - the game's item
//! document for its own thirty, and mods' merges over it last - plus
//! `item_recipes.json`, which a code mod writes as its items register (the
//! Riot pack does). Nothing else reaches those items' paths, and the links
//! from the game's items to the Riot pack's components are only written on
//! the components' side, as `previous_tier`.
//!
//! The exe says which item the card shows only by its name, so the item is
//! found by name, matched against the names on the grid's own slots - which
//! are in the player's language, whatever it is, and each slot is named after
//! its item. An item it cannot name, or price, keeps the exe's own number.

use mod_api_stable::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

const MOD_ID: &str = "item_scroller_tfm2";

/// The exe addresses the grid as `data.item_list.contents`, but the screen's
/// root prefix is not part of the stable contract, so we search for the node.
const LIST_NODE: &str = "item_list";
const CONTENTS_NODE: &str = "contents";
/// The item art and the name label inside a slot, from the `item_slot`
/// template.
const SLOT_ICON: &str = "icon";
const SLOT_NAME: &str = "name";

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
/// A mod's merge over the game's item document, relative to its folder, and the
/// asset its `mod.override_info` has to name for the game to apply it.
const MOD_ITEM_SETTING: &str = "setting/item_setting.item_setting";
const MOD_OVERRIDE_INFO: &str = "mod.override_info";
const ITEM_SETTING_ASSET: &str = "asset/base/setting/item_setting";
/// Convention filename for a code mod that writes its items' prices and build
/// paths out as data: `{ "<id>": { "price", "previous_tier", "next_tier" } }`.
const MOD_ITEM_PATHS: &str = "item_recipes.json";
/// How a label refers to an item's name for the game to resolve.
const ITEM_NAME_PREFIX: &str = "#asset/base/text/item?";
const ITEM_NAME_SUFFIX: &str = ".name";

/// The card's name and price labels, under the screen's `data` node - the exe
/// writes `data.item_detail.name` and `data.item_detail.price_area.price`.
const DETAIL_NAME: &str = "item_detail.name";
const DETAIL_PRICE: &str = "item_detail.price_area.price";
/// Upgrade steps a path is followed for at most: more than there are tiers,
/// and what ends a path that runs in a circle.
const MAX_STEPS: usize = 8;

/// The item pack whose classes the class menu lists: its folder name under
/// `mods/` when installed by hand, and its Workshop published file id when
/// subscribed.
const PACK_ID: &str = "riot_items_tfm2";
const PACK_WORKSHOP_ID: &str = "3739568852";
/// The pack's item -> class table, relative to its mod folder.
const PACK_CLASS_FILE: &str = "text/item_classes.json";

/// The game's own dropdown chevron and its flipped twin. Both sprites are
/// 8.78x5.06, so swapping one for the other never moves or resizes the caret.
const CARET_DOWN: &str = "source: \"asset/base/ui/icons/dropdown\";";
const CARET_UP: &str = "source: \"asset/base/ui/icons/dropdown_up\";";

const DIM_SLOT: &str = "disable: true;";
const LIT_SLOT: &str = "disable: false;";
const DIM_ICON: &str = "color: #ffffff59;";
const LIT_ICON: &str = "color: #ffffffff;";

/// `riot_items_tfm2`'s boots, in the order the Boots section shows them: the
/// tier-1 pair first, then its upgrades alphabetically.
const BOOTS: [&str; 8] = [
    "boots",
    "berserkers_greaves",
    "boots_of_swiftness",
    "gluttonous_greaves",
    "ionian_boots_of_lucidity",
    "mercurys_treads",
    "plated_steelcaps",
    "sorcerers_shoes",
];
/// The Boots section's header node. The exe names its own `<tier>_header`.
const BOOTS_HEADER: &str = "boots_header";
/// The header's text, which `riot_items_tfm2` ships beside the tier names it
/// already renames (`tier_header.1` .. `tier_header.5`).
const BOOTS_HEADER_TEXT: &str = "#asset/base/text/item?tier_header.boots";
/// The sheet every item icon is cut from, by `rect_tag` = the item's icon key.
const ICON_SHEET: &str = "asset/base/aseprite_resources/ingame/item_icons_18x18";

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
    ("Ability Haste", &["skill_cooldown_mult"]),
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
/// is a class code, offset by one. The names are what the pack's class file
/// spells them as.
const CLASSES: [&str; 7] = [
    "All Classes",
    "Assassin",
    "Fighter",
    "Tank",
    "Mage",
    "Marksman",
    "Support",
];

/// item slug -> class code, which indexes [`CLASSES`] minus its first entry.
type ClassTable = BTreeMap<String, u8>;

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

/// The class of an item the pack itself adds - and so also the test for whether
/// the pack is installed at all, since every slug in the table is one of its
/// items. A `radiant_` upgrade carries its base item's id under the prefix.
fn pack_class(classes: &ClassTable, id: &str) -> Option<u8> {
    classes
        .get(id.strip_prefix("radiant_").unwrap_or(id))
        .copied()
}

/// The class of any item in the grid, the reskinned base items included.
fn class_of(classes: &ClassTable, id: &str) -> Option<u8> {
    pack_class(classes, id).or_else(|| {
        RESKINNED
            .iter()
            .find(|(key, _)| *key == id)
            .and_then(|(_, slug)| classes.get(*slug).copied())
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

/// The id a settings entry is filed under.
///
/// The map key and the entry's own `key` agree for all but one base item -
/// `iron_blade` is `ironsword` - and it is the inner `key` the grid names its
/// slots by, so that is what we file under.
fn setting_id<'a>(map_key: &'a str, entry: &'a serde_json::Value) -> &'a str {
    entry
        .get("key")
        .and_then(|value| value.as_str())
        .filter(|key| !key.is_empty())
        .unwrap_or(map_key)
}

/// Every item in the game's item document - `{ "<id>": { "key": "<id>",
/// "stat": { ... } }, "mod_items": [...] }` - or in a mod's merge over it,
/// which has the same shape with fewer items and fields, by the id it is
/// filed under.
fn each_setting_item(document: &str, mut visit: impl FnMut(&str, &serde_json::Value)) {
    let document = document.trim_start_matches('\u{feff}');
    let Ok(root) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(document)
    else {
        return;
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
                    visit(id, entry);
                }
            }
        } else {
            visit(setting_id(id, entry), entry);
        }
    }
}

/// The game's item document.
fn absorb_item_setting(document: &str, items: &mut ItemStats) {
    each_setting_item(document, |id, entry| {
        let granted: Vec<String> = entry
            .get("stat")
            .and_then(|stat| stat.as_object())
            .map(|stat| {
                stat.iter()
                    .filter(|(_, value)| nonzero(value))
                    .map(|(key, _)| key.clone())
                    .collect()
            })
            .unwrap_or_default();
        record(items, id, granted.into_iter());
    });
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

/// A mod's merge over the game's item document - the same shape, usually with
/// only some items, and possibly only some of their stats. Each stat it names
/// replaces the one already read, and the rest are left alone, as the game's
/// merge does. Replacing rather than adding matters: the Riot pack takes
/// Health off the Null-Magic Mantle (`mystic_cloak`) as well as giving its
/// Phantom Dancer (`thunderclaw`) movement speed.
fn absorb_setting_merge(document: &str, items: &mut ItemStats) {
    each_setting_item(document, |id, entry| {
        let Some(stat) = entry.get("stat").and_then(|stat| stat.as_object()) else {
            return;
        };
        let granted = items.entry(id.to_string()).or_default();
        for (key, value) in stat {
            granted.retain(|had| had != key);
            if nonzero(value) {
                granted.push(key.clone());
            }
        }
    });
}

/// Whether a mod's `mod.override_info` merges one of its files over the game's
/// `asset`. Only then does the game apply it - and only then do we, which keeps
/// a settings file that is merely lying in a folder (an unpacked copy of the
/// base game's, say) from being read as a mod's.
fn merges(override_info: &str, asset: &str) -> bool {
    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(
        override_info.trim_start_matches('\u{feff}'),
    )
    .is_ok_and(|root| root.contains_key(asset))
}

/// Where subscribed Workshop items live: beside the game install rather than
/// inside it, one folder per published file id.
fn workshop_root(game: &Path) -> Option<PathBuf> {
    // ...steamapps/common/<game> -> ...steamapps/workshop/content/<app id>
    let steamapps = game.parent()?.parent()?;
    Some(steamapps.join("workshop").join("content").join(APP_ID))
}

/// Mod folders to look in: the game's own `mods/`, and subscribed Workshop
/// items.
fn mod_roots() -> Vec<PathBuf> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    let mut roots = vec![cwd.join("mods")];
    roots.extend(workshop_root(&cwd));
    roots
}

/// Where the item pack can be installed: by hand under its mod id, or from the
/// Workshop under its published file id. The manual copy comes first - with
/// both present, it is the one being worked on.
fn pack_dirs() -> Vec<PathBuf> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    let mut dirs = vec![cwd.join("mods").join(PACK_ID)];
    dirs.extend(workshop_root(&cwd).map(|root| root.join(PACK_WORKSHOP_ID)));
    dirs
}

/// The pack's class file: `{ "<slug>": "<class>" }`. Entries whose class is
/// not one the menu lists - `Boots`, chiefly - are dropped.
fn parse_class_file(document: &str) -> ClassTable {
    let document = document.trim_start_matches('\u{feff}');
    let Ok(root) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(document)
    else {
        return ClassTable::new();
    };
    root.iter()
        .filter_map(|(slug, class)| {
            let class = class.as_str()?;
            let code = CLASSES[1..].iter().position(|name| *name == class)?;
            Some((slug.clone(), code as u8))
        })
        .collect()
}

/// The first class file found in [`pack_dirs`], or an empty table.
fn load_class_file() -> ClassTable {
    pack_dirs()
        .into_iter()
        .find_map(|dir| std::fs::read_to_string(dir.join(PACK_CLASS_FILE)).ok())
        .map_or_else(ClassTable::new, |text| parse_class_file(&text))
}

/// Every mod's folder, in [`mod_roots`].
fn mod_dirs() -> Vec<PathBuf> {
    mod_roots()
        .into_iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flat_map(|entries| entries.flatten().map(|entry| entry.path()))
        .collect()
}

/// A mod's `file`, if its `mod.override_info` merges it over the game's `asset`.
fn merged_file(dir: &Path, asset: &str, file: &str) -> Option<String> {
    let override_info = std::fs::read_to_string(dir.join(MOD_OVERRIDE_INFO)).ok()?;
    if !merges(&override_info, asset) {
        return None;
    }
    std::fs::read_to_string(dir.join(file)).ok()
}

fn absorb_mod_files(dirs: &[PathBuf], items: &mut ItemStats) {
    for dir in dirs {
        if let Ok(text) = std::fs::read_to_string(dir.join(MOD_ITEM_CONFIG)) {
            absorb_mod_config(&text, items);
        }
    }
    // Merges go last: they are what the game actually applies to the items
    // they name, so they get the final word on those items' stats.
    for dir in dirs {
        if let Some(text) = merged_file(dir, ITEM_SETTING_ASSET, MOD_ITEM_SETTING) {
            absorb_setting_merge(&text, items);
        }
    }
}

/// Prices and build paths from mods: their merges over the game's item
/// document, then the paths code mods write out. A mod installed both by hand
/// and from the Workshop can have written two, so those are read oldest first
/// and the copy that wrote last has the final word.
fn absorb_mod_paths(dirs: &[PathBuf], entries: &mut PathEntries) {
    for dir in dirs {
        if let Some(text) = merged_file(dir, ITEM_SETTING_ASSET, MOD_ITEM_SETTING) {
            each_setting_item(&text, |id, entry| absorb_path_entry(entries, id, entry));
        }
    }
    let mut written: Vec<(SystemTime, String)> = dirs
        .iter()
        .filter_map(|dir| {
            let file = dir.join(MOD_ITEM_PATHS);
            let modified = std::fs::metadata(&file).and_then(|meta| meta.modified()).ok()?;
            Some((modified, std::fs::read_to_string(&file).ok()?))
        })
        .collect();
    written.sort_by_key(|(modified, _)| *modified);
    for (_, text) in &written {
        absorb_path_file(text, entries);
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
    classes: &ClassTable,
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
        let class_dim = class.is_some_and(|wanted| class_of(classes, &child) != Some(wanted));
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

// --- the Boots section ----------------------------------------------------

/// One rebuilt item slot: the exe's `item_info_component/item_slot` template,
/// named by the item id the way the exe names its own, with the icon and name
/// the exe would have filled in.
fn boots_slot_source(id: &str) -> String {
    format!(
        "{id}:color_icon_button {{ @\"asset/base/style/main#tertiary_button\"; \
           width: 117px; height: 124px; rounding: 12; \
           #{SLOT_ICON}:image {{ width: 64px; height: 64px; anchor_x: 0.5; pivot_x: 0.5; \
             y: 10px; ignore_event: true; source: \"{ICON_SHEET}\"; rect_tag: \"{id}\"; }} \
           #name_bg:color {{ width: 115px; height: 41px; anchor_x: 0.5; pivot_x: 0.5; \
             anchor_y: 1; pivot_y: 1; y: -1px; color: #0f1016ff; \
             rounding: Individual {{ top_left: 0; top_right: 0; bottom_left: 12; \
               bottom_right: 12; }} ignore_event: true; }} \
           #name:label {{ @\"asset/base/style/main#label\"; width: 112px; height: 41px; \
             anchor_x: 0.5; pivot_x: 0.5; anchor_y: 1; pivot_y: 1; \
             align_x: Center; align_y: Center; size: 14; \
             text: \"#asset/base/text/item?{id}.name\"; }} }}"
    )
}

/// The Boots section's header: the exe's `item_info_component/tier_header`.
fn boots_header_source() -> String {
    format!(
        "{BOOTS_HEADER}:empty {{ width: 1010px; height: 32px; \
           #text:label {{ @\"asset/base/style/main#label\"; width: 100%; height: 32px; \
             size: 16; color: #e8e8e8ff; align_y: Center; text: \"{BOOTS_HEADER_TEXT}\"; }} }}"
    )
}

/// Moves every boots slot in the grid into a Boots section after the last tier.
/// Returns whether it changed the grid, which it does not when no boots are in
/// it (the pack is not installed) or the grid has not been populated yet.
///
/// The originals go first: sibling names are unique, and the rebuilt slots take
/// the same ones.
fn regroup_boots(ctx: &mut StableClient<'_>, contents: &str) -> bool {
    let children = ctx.ui_child_names(contents);
    if children.iter().any(|name| name == BOOTS_HEADER) {
        return false;
    }
    let present: Vec<&str> = BOOTS
        .iter()
        .copied()
        .filter(|id| children.iter().any(|name| name == id))
        .collect();
    if present.is_empty() {
        return false;
    }
    for id in &present {
        ctx.ui_remove_node(&join(contents, id));
    }
    ctx.ui_spawn_source(contents, &boots_header_source());
    for id in &present {
        ctx.ui_spawn_source(contents, &boots_slot_source(id));
    }
    true
}

// --- the total price ------------------------------------------------------

/// What is known of one item's place in the build paths. A source may give
/// any of the fields, and each one it gives replaces what was read before.
#[derive(Default)]
struct PathEntry {
    price: Option<u32>,
    previous: Vec<String>,
    next: Vec<String>,
}

type PathEntries = BTreeMap<String, PathEntry>;

/// item id -> its own price and the items it can be built from, any one of
/// them. Made by [`link`].
type Paths = BTreeMap<String, (u32, Vec<String>)>;

/// An item's name, in any language read -> its id.
type ItemNames = BTreeMap<String, String>;

/// One item's `price`, `previous_tier` and `next_tier`, from any source that
/// gives them: the game's item document, a mod's merge over it, or a code
/// mod's [`MOD_ITEM_PATHS`].
fn absorb_path_entry(entries: &mut PathEntries, id: &str, entry: &serde_json::Value) {
    let ids = |field: &str| {
        entry.get(field).and_then(|value| value.as_array()).map(|list| {
            list.iter()
                .filter_map(|id| id.as_str())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
    };
    let known = entries.entry(id.to_string()).or_default();
    if let Some(price) = entry.get("price").and_then(|price| price.as_f64()) {
        known.price = Some(price.max(0.0).round() as u32);
    }
    if let Some(previous) = ids("previous_tier") {
        known.previous = previous;
    }
    if let Some(next) = ids("next_tier") {
        known.next = next;
    }
}

/// A code mod's [`MOD_ITEM_PATHS`]: `{ "<id>": { "price": 650,
/// "previous_tier": [...], "next_tier": [...] } }`.
fn absorb_path_file(document: &str, entries: &mut PathEntries) {
    let document = document.trim_start_matches('\u{feff}');
    let Ok(root) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(document)
    else {
        return;
    };
    for (id, entry) in &root {
        absorb_path_entry(entries, id, entry);
    }
}

/// Joins every link at both ends. The game's items name only what they are
/// upgraded into, and a mod's component built from one of them names only
/// what it is built from, so an item's sources are its own `previous_tier`
/// plus every item whose `next_tier` names it. A link to an item with no price,
/// such as one from a mod that is not installed, leads nowhere and is dropped.
fn link(entries: &PathEntries) -> Paths {
    let mut paths: Paths = entries
        .iter()
        .filter_map(|(id, entry)| Some((id.clone(), (entry.price?, Vec::new()))))
        .collect();
    let links = entries.iter().flat_map(|(id, entry)| {
        let from = entry.previous.iter().map(move |previous| (previous, id));
        let into = entry.next.iter().map(move |next| (id, next));
        from.chain(into)
    });
    for (from, into) in links {
        if !paths.contains_key(from) {
            continue;
        }
        if let Some((_, sources)) = paths.get_mut(into) {
            if !sources.contains(from) {
                sources.push(from.clone());
            }
        }
    }
    paths
}

/// The least and the most an item costs from nothing: its own price, plus the
/// cheapest and the dearest way up to any item it is built from. Followed for
/// at most `steps` items, which is what ends a path that runs in a circle.
fn total_price(paths: &Paths, id: &str, steps: usize) -> Option<(u32, u32)> {
    let (price, sources) = paths.get(id)?;
    let below = if steps > 1 {
        sources
            .iter()
            .filter_map(|source| total_price(paths, source, steps - 1))
            .reduce(|(low, high), (other_low, other_high)| {
                (low.min(other_low), high.max(other_high))
            })
    } else {
        None
    };
    let (low, high) = below.unwrap_or((0, 0));
    Some((price + low, price + high))
}

/// A total as the card shows it: one number, or a range when the ways up cost
/// different amounts.
fn price_text((low, high): (u32, u32)) -> String {
    if low == high {
        low.to_string()
    } else {
        format!("{low} - {high}")
    }
}

/// Every slot's name in the grid, by the item id the slot is named after. The
/// exe fills those labels the way it fills the card's, in the player's
/// language, so whatever the card says is in here. Nothing else knows the game
/// items' names outside English: `i18n` only ever answers in English.
///
/// Read before [`regroup_boots`] replaces the exe's boots slots, whose rebuilt
/// labels hold only a reference. Names are added, never cleared, so a name
/// read from the exe's slot outlives that slot.
fn index_grid_names(ctx: &StableClient<'_>, contents: &str, names: &mut ItemNames) {
    for id in ctx.ui_child_names(contents) {
        let Some(name) = ctx.ui_text(&join(&join(contents, &id), SLOT_NAME)) else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty() {
            names.insert(name.to_string(), id);
        }
    }
}

/// The item a name label shows: by the reference it was handed for the game
/// to resolve, or by the name that reference resolved to.
fn item_named<'a>(paths: &'a Paths, names: &'a ItemNames, label: &str) -> Option<&'a str> {
    let reference = label
        .strip_prefix(ITEM_NAME_PREFIX)
        .and_then(|path| path.strip_suffix(ITEM_NAME_SUFFIX));
    match reference {
        Some(id) => paths.get_key_value(id).map(|(id, _)| id.as_str()),
        None => names.get(label.trim()).map(String::as_str),
    }
}

/// What the card's price label was last set to, and for which item.
struct PriceShown {
    /// The name label at the time.
    name: String,
    /// The exe's own price text for that item.
    game: String,
    /// What the label was set to.
    shown: String,
}

/// Puts the total price on the card in place of the step price the exe
/// writes, whenever the label stops saying what was last put there - which
/// covers the exe filling it once per item or every frame. An item that
/// cannot be named or priced keeps the exe's text.
fn show_total(state: &mut State, ctx: &mut StableClient<'_>, host: &str) {
    let label = join(host, DETAIL_PRICE);
    let Some(text) = ctx.ui_text(&label) else {
        return;
    };
    let name = ctx.ui_text(&join(host, DETAIL_NAME)).unwrap_or_default();
    let game = match &state.price {
        Some(last) if last.shown == text => {
            if last.name == name {
                return;
            }
            // The card moved on to another item but the exe left the label
            // as it was, so that item's price is the one the exe last wrote.
            last.game.clone()
        }
        _ => text.clone(),
    };
    let shown = item_named(&state.paths, &state.names, &name)
        .and_then(|id| total_price(&state.paths, id, MAX_STEPS))
        .map_or_else(|| game.clone(), price_text);
    if shown != text {
        ctx.ui_set_text(&label, &shown);
    }
    state.price = Some(PriceShown { name, game, shown });
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
    /// The pack's item -> class table, empty when its file was not found.
    classes: ClassTable,
    loaded: bool,
    /// Whether the item pack that defines the classes is installed, which is
    /// also whether the class menu is on screen.
    classed: bool,
    applied: Option<([usize; MENU_COUNT], usize)>,
    /// Last `y` written by `place`, so the properties are only rewritten when
    /// the control actually has to move.
    placed: Option<i32>,
    /// Whether [`regroup_boots`] has rebuilt the current grid's Boots section.
    regrouped: bool,
    /// Every item's price and what it is built from, for [`show_total`].
    paths: Paths,
    names: ItemNames,
    /// What [`show_total`] last put on the card.
    price: Option<PriceShown>,
}

struct ItemFilter {
    state: Mutex<State>,
}

impl ItemFilter {
    /// Reads every source of item stats, classes and prices. None of them can
    /// change while the game runs, so this happens once.
    fn load(state: &mut State, ctx: &StableClient<'_>) {
        let mut entries = PathEntries::new();
        if let Some(document) = ctx.setting_get_json(SettingTargetV1::ItemSetting, "") {
            absorb_item_setting(&document, &mut state.items);
            each_setting_item(&document, |id, entry| {
                absorb_path_entry(&mut entries, id, entry)
            });
        }
        let dirs = mod_dirs();
        absorb_mod_files(&dirs, &mut state.items);
        absorb_mod_paths(&dirs, &mut entries);
        state.paths = link(&entries);
        state.classes = load_class_file();
    }

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
    /// in its class file is one of the pack's own items.
    fn detect_classes(state: &mut State, ctx: &mut StableClient<'_>, root: &str, contents: &str) {
        let classed = ctx
            .ui_child_names(contents)
            .iter()
            .any(|name| pack_class(&state.classes, name).is_some());
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
            state.regrouped = false;
            state.price = None;
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

        if !state.loaded {
            state.loaded = true;
            Self::load(&mut state, ctx);
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
        let mut count = ctx.ui_child_count(&contents).unwrap_or(0);
        if state.applied.is_none_or(|(_, applied)| applied != count) {
            Self::detect_classes(&mut state, ctx, &root, &contents);
            // Before the boots are regrouped, while their slots are the exe's.
            index_grid_names(ctx, &contents, &mut state.names);
            // A repopulated grid has lost the Boots section along with every
            // other child, so it is rebuilt whenever the header is gone.
            if state.regrouped && !ctx.ui_exists(&join(&contents, BOOTS_HEADER)) {
                state.regrouped = false;
            }
            // Only for `riot_items_tfm2`'s boots: `classed` is set by its own
            // legendaries being in the grid, so another mod's item that happens
            // to be called `boots` is never moved.
            if state.classed && !state.regrouped && regroup_boots(ctx, &contents) {
                state.regrouped = true;
                count = ctx.ui_child_count(&contents).unwrap_or(0);
            }
        }

        let changed = Self::poll(&mut state, ctx, &root);

        if changed || state.applied != Some((state.current, count)) {
            let keys = FILTERS[state.current[STAT]].1;
            let class = class_filter(state.current[CLASS]);
            apply(&state.items, &state.classes, ctx, &contents, keys, class);
            state.applied = Some((state.current, count));
        }

        show_total(&mut state, ctx, &host);
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

    const SAMPLE: &str = "\u{feff}{ \"hubris\": \"Assassin\", \"thornmail\": \"Tank\", \
         \"plated_steelcaps\": \"Boots\", \"odd\": \"Jungler\", \"bad\": 3 }";

    #[test]
    fn class_file_parses() {
        let classes = parse_class_file(SAMPLE);
        // Boots, a class the menu does not list, and a non-string are dropped.
        assert_eq!(classes.len(), 2);
        assert_eq!(classes.get("hubris"), Some(&0));
        assert_eq!(classes.get("thornmail"), Some(&2));
        assert!(parse_class_file("not json").is_empty());
    }

    #[test]
    fn classes_resolve() {
        let classes = parse_class_file(SAMPLE);
        // The pack's own item, its radiant upgrade, and a base item the pack
        // reskins as a radiant - the last classified, but never proof the pack
        // is installed.
        assert_eq!(class_of(&classes, "hubris"), Some(0));
        assert_eq!(class_of(&classes, "radiant_thornmail"), Some(2));
        assert_eq!(class_of(&classes, "impregnable_fortress"), Some(2));
        assert!(pack_class(&classes, "impregnable_fortress").is_none());
        // A base item and one of the pack's components have no class at all.
        assert!(class_of(&classes, "iron_blade").is_none());
        assert!(class_of(&classes, "bf_sword").is_none());
    }

    /// The pack's real class file, when it is checked out beside this repo. A
    /// class the pack adds would be silently dropped by the parser, so it has
    /// to be caught here and given a row in [`CLASSES`].
    #[test]
    fn pack_class_file_is_covered() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(PACK_ID)
            .join(PACK_CLASS_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let root: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap();
        for (slug, class) in &root {
            let class = class.as_str().unwrap();
            assert!(
                class == "Boots" || CLASSES[1..].contains(&class),
                "{slug}: class {class:?} has no row in CLASSES"
            );
        }
        let classes = parse_class_file(&text);
        for (base, slug) in RESKINNED {
            assert!(classes.contains_key(slug), "{base} reskins unclassed {slug}");
        }
    }

    #[test]
    fn setting_merge_replaces_stats() {
        let mut items = ItemStats::new();
        absorb_item_setting(
            r#"{ "thunderclaw": { "key": "thunderclaw",
                   "stat": { "attack_speed_mult": 45, "move_speed_mult": 0 } },
                 "mystic_cloak": { "key": "mystic_cloak",
                   "stat": { "hp": 50, "magic_resistance": 20 } },
                 "iron_blade": { "key": "ironsword", "stat": { "attack": 5 } } }"#,
            &mut items,
        );
        // Partial entries, as a merge may be: only the stats it names change.
        absorb_setting_merge(
            r#"{ "thunderclaw": { "stat": { "move_speed_mult": 5 } },
                 "mystic_cloak": { "stat": { "hp": 0 } },
                 "iron_blade": { "key": "ironsword", "stat": { "crit_chance": 5 } } }"#,
            &mut items,
        );
        let has = |id: &str, key: &str| items[id].iter().any(|had| had == key);
        assert!(has("thunderclaw", "attack_speed_mult") && has("thunderclaw", "move_speed_mult"));
        assert!(!has("mystic_cloak", "hp") && has("mystic_cloak", "magic_resistance"));
        assert!(has("ironsword", "attack") && has("ironsword", "crit_chance"));
    }

    #[test]
    fn only_wired_merges_count() {
        assert!(merges(
            r#"{ "asset/base/setting/item_setting": { "type": "merge" } }"#,
            ITEM_SETTING_ASSET
        ));
        assert!(!merges(r#"{ "asset/base/text/item": {} }"#, ITEM_SETTING_ASSET));
        assert!(!merges("not json", ITEM_SETTING_ASSET));
    }

    #[test]
    fn totals_follow_every_way_up() {
        let mut entries = PathEntries::new();
        // The game's own items, linked by `next_tier` only.
        each_setting_item(
            r#"{ "iron_blade": { "key": "ironsword", "price": 500,
                   "next_tier": ["soldiers_longsword"] },
                 "soldiers_longsword": { "key": "soldiers_longsword", "price": 500,
                   "next_tier": ["ruinous_blade"] },
                 "ruinous_blade": { "key": "ruinous_blade", "price": 800, "next_tier": [] },
                 "vital_orb": { "key": "vital_orb", "price": 500,
                   "next_tier": ["hardened_heart"] },
                 "hardened_heart": { "key": "hardened_heart", "price": 500, "next_tier": [] } }"#,
            |id, entry| absorb_path_entry(&mut entries, id, entry),
        );
        // A mod's merge over them: prices only, so the links stay.
        each_setting_item(
            r#"{ "iron_blade": { "key": "ironsword", "price": 250 },
                 "soldiers_longsword": { "price": 250 },
                 "ruinous_blade": { "price": 400 },
                 "vital_orb": { "price": 250 },
                 "hardened_heart": { "price": 250 } }"#,
            |id, entry| absorb_path_entry(&mut entries, id, entry),
        );
        // A code mod's items, linked to the game's by `previous_tier` only.
        absorb_path_file(
            "\u{feff}{ \"serrated_dirk\": { \"price\": 400, \
               \"previous_tier\": [\"soldiers_longsword\"], \"next_tier\": [\"bastionbreaker\"] }, \
             \"bastionbreaker\": { \"price\": 650, \"previous_tier\": [\"serrated_dirk\"], \
               \"next_tier\": [] }, \
             \"phage\": { \"price\": 500, \
               \"previous_tier\": [\"hardened_heart\", \"ruinous_blade\"], \"next_tier\": [] }, \
             \"orphan\": { \"price\": 100, \"previous_tier\": [\"not_installed\"] } }",
            &mut entries,
        );
        let paths = link(&entries);
        let total = |id: &str| total_price(&paths, id, MAX_STEPS);

        assert_eq!(total("ironsword"), Some((250, 250)));
        assert_eq!(total("ruinous_blade"), Some((900, 900)));
        // 650 + Serrated Dirk 400 + Long Sword 250 + its Starter 250.
        assert_eq!(total("bastionbreaker"), Some((1550, 1550)));
        // From the 500 Heart, or the 900 Blade.
        assert_eq!(total("phage"), Some((1000, 1400)));
        assert_eq!(total("orphan"), Some((100, 100)));
        assert_eq!(total("not_installed"), None);

        assert_eq!(price_text((1550, 1550)), "1550");
        assert_eq!(price_text((1000, 1400)), "1000 - 1400");
    }

    #[test]
    fn a_circle_ends() {
        let mut entries = PathEntries::new();
        absorb_path_file(
            r#"{ "a": { "price": 1, "previous_tier": ["b"] },
                 "b": { "price": 1, "previous_tier": ["a"] } }"#,
            &mut entries,
        );
        assert_eq!(total_price(&link(&entries), "a", MAX_STEPS), Some((8, 8)));
    }

    #[test]
    fn items_are_found_by_name() {
        // As the grid gives them: whatever language its labels are in.
        let names: ItemNames = [
            ("Quebra-Bastião".to_string(), "bastionbreaker".to_string()),
            ("Long Sword".to_string(), "ironsword".to_string()),
        ]
        .into();
        let paths: Paths = [("bastionbreaker".to_string(), (650, Vec::new()))].into();
        let named = |label: &str| item_named(&paths, &names, label);

        assert_eq!(named("Quebra-Bastião "), Some("bastionbreaker"));
        assert_eq!(named("Long Sword"), Some("ironsword"));
        assert_eq!(
            named("#asset/base/text/item?bastionbreaker.name"),
            Some("bastionbreaker")
        );
        assert_eq!(named("#asset/base/text/item?unknown.name"), None);
        assert_eq!(named("Price"), None);
    }

    #[test]
    fn boots_section_sources() {
        assert_eq!(BOOTS[0], "boots");
        let slot = boots_slot_source("mercurys_treads");
        assert!(slot.starts_with("mercurys_treads:color_icon_button"));
        assert!(slot.contains("rect_tag: \"mercurys_treads\""));
        assert!(boots_header_source().starts_with("boots_header:empty"));
    }
}

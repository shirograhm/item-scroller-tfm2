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
const HEAD_NODE: &str = "head";
const CARET_NODE: &str = "caret";
const PANEL_NODE: &str = "panel";

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
/// touch our state. `usize::MAX` means "nothing clicked since last read".
static CLICKED_ROW: AtomicUsize = AtomicUsize::new(usize::MAX);
static CLICKED_HEAD: AtomicBool = AtomicBool::new(false);

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

/// The panel paints its own background, and a painted node needs a real height.
///
/// With `height: auto` the fill covered only the first row while the rest of the
/// list drew straight over the item card behind it. Every other `auto` in this
/// mod is on an `:empty` or `:label` node, and the panel's parent
/// (`item_filter`) is a fixed 40px, so `auto` has nothing to grow against.
/// Computed rather than hardcoded so adding a filter cannot silently re-open the
/// same gap.
fn panel_height() -> u32 {
    let rows = FILTERS.len() as u32;
    rows * ROW_HEIGHT + rows.saturating_sub(1) * ROW_SPACING + PANEL_PADDING * 2
}

/// How tall the root has to be for the open panel to fit inside it.
fn root_open_height() -> u32 {
    PANEL_Y + panel_height()
}

/// Shows or hides the option panel.
///
/// The panel is a child of the root, and the root is a fixed 40px strip, so the
/// panel's painted background was being clipped to that 40px — covering only the
/// first row while the `color_selectable` rows, which draw separately and carry
/// no `z`, spilled out over the item card behind. Giving the panel its own
/// height was not enough on its own; the root has to make room for it. It is
/// restored on close so the collapsed control still occupies just its own strip
/// and never sits over the card.
fn set_open(ctx: &mut StableClient<'_>, root: &str, open: bool) {
    ctx.ui_set_visible(&join(root, PANEL_NODE), open);
    let caret = join(&join(root, HEAD_NODE), CARET_NODE);
    ctx.ui_set_properties(&caret, if open { CARET_UP } else { CARET_DOWN });
    let height = if open {
        root_open_height()
    } else {
        ROOT_HEIGHT
    };
    ctx.ui_set_properties(root, &format!("height: {height}px;"));
}

fn control_source() -> String {
    let mut rows = String::new();
    for (index, (label, _)) in FILTERS.iter().enumerate() {
        rows.push_str(&format!(
            "#opt{index}:color_selectable {{ @\"asset/base/style/main#strategy_option\"; \
             width: {ROW_WIDTH}px; height: {ROW_HEIGHT}px; \
             label: {{ size: 14; }} selected_label: {{ size: 14; }} \
             text: \"{label}\"; }} "
        ));
    }

    let panel = panel_height();
    // Where the control lands on the full Game Info screen; `place` corrects it
    // from the card's real position as soon as there is a layout pass to read.
    let spawn_y = CARD_Y as i32 - (ROOT_HEIGHT + CONTROL_GAP) as i32;
    format!(
        "{ROOT_NODE}:empty {{ x: {ROOT_X}px; y: {spawn_y}px; \
         width: {ROOT_WIDTH}px; height: {ROOT_HEIGHT}px; \
         #{HEAD_NODE}:color_selectable {{ @\"asset/base/style/main#strategy_option\"; \
           anchor_x: 1; pivot_x: 1; width: {HEAD_WIDTH}px; height: {HEAD_HEIGHT}px; \
           image: {{ color: #4a4c56ff; back_color: #1d1f2cff; stroke: 1; \
                     rounding: Uniform {{ rounding: 8; }} \
                     hover: {{ color: #a5a5abff; }} }} \
           label: {{ size: 15; }} selected_label: {{ size: 15; }} \
           text: \"All Items\"; \
           #{CARET_NODE}:image {{ {CARET_DOWN} ignore_event: true; color: #a5a5abff; \
             anchor_x: 1; pivot_x: 1; x: -{CARET_INSET}px; \
             anchor_y: 0.5; pivot_y: 0.5; width: 8.78px; height: 5.06px; }} }} \
         #{PANEL_NODE}:color {{ anchor_x: 1; pivot_x: 1; y: {PANEL_Y}px; \
           width: {PANEL_WIDTH}px; height: {panel}px; visible: false; \
           color: #4a4c56ff; back_color: #161721ff; stroke: 1; \
           rounding: Uniform {{ rounding: 8; }} \
           padding: {{ left: {PANEL_PADDING}px; right: {PANEL_PADDING}px; \
                       top: {PANEL_PADDING}px; bottom: {PANEL_PADDING}px; }} \
           child_type: TopToBottom {{ spacing: {ROW_SPACING}px; }} \
           {rows} }} }}"
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

/// Applies the filter by graying non-matching slots. Slots we have no stats for
/// are left lit.
fn apply(items: &ItemStats, ctx: &mut StableClient<'_>, contents: &str, keys: &[&str]) {
    let mut unresolved: Vec<String> = Vec::new();

    for child in ctx.ui_child_names(contents) {
        let dim = !keys.is_empty()
            && match items.get(&child) {
                Some(granted) => !keys.iter().any(|key| granted.iter().any(|had| had == key)),
                None => {
                    unresolved.push(child.clone());
                    false
                }
            };

        let slot = join(contents, &child);
        ctx.ui_set_properties(&slot, if dim { DIM_SLOT } else { LIT_SLOT });
        ctx.ui_set_properties(
            &join(&slot, SLOT_ICON),
            if dim { DIM_ICON } else { LIT_ICON },
        );
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
    open: bool,
    current: usize,
    items: ItemStats,
    loaded: bool,
    applied: Option<(usize, usize)>,
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

        let root = join(host, ROOT_NODE);
        ctx.ui_register_click(&join(&root, HEAD_NODE), "", |_| {
            CLICKED_HEAD.store(true, Ordering::Relaxed);
        });
        let panel = join(&root, PANEL_NODE);
        for index in 0..FILTERS.len() {
            ctx.ui_register_click(&join(&panel, &format!("opt{index}")), "", move |_| {
                CLICKED_ROW.store(index, Ordering::Relaxed);
            });
        }
    }

    /// Clicks arrive either through a registered handler or as the row's own
    /// `selected` flag; both are drained here, and the flag is cleared so the
    /// rows never look stuck.
    fn poll(state: &mut State, ctx: &mut StableClient<'_>, root: &str) -> bool {
        let head = join(root, HEAD_NODE);
        let head_selected = ctx.ui_selectable_selected(&head).unwrap_or(false);
        if head_selected {
            ctx.ui_set_selectable_selected(&head, false);
        }
        if CLICKED_HEAD.swap(false, Ordering::Relaxed) || head_selected {
            state.open = !state.open;
            set_open(ctx, root, state.open);
        }

        let panel = join(root, PANEL_NODE);
        let mut chosen = match CLICKED_ROW.swap(usize::MAX, Ordering::Relaxed) {
            usize::MAX => None,
            index => Some(index),
        };
        for index in 0..FILTERS.len() {
            let row = join(&panel, &format!("opt{index}"));
            if ctx.ui_selectable_selected(&row).unwrap_or(false) {
                ctx.ui_set_selectable_selected(&row, false);
                chosen = Some(index);
            }
        }

        let Some(index) = chosen.filter(|index| *index < FILTERS.len()) else {
            return false;
        };

        state.current = index;
        state.open = false;
        set_open(ctx, root, false);
        let label = FILTERS[index].0;
        ctx.ui_set_properties(&head, &format!("text: \"{label}\";"));
        ctx.ui_set_text(&head, label);
        true
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
            state.open = false;
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

        let changed = Self::poll(&mut state, ctx, &root);

        // The game repopulates the grid when the tab is reopened, so reapply on
        // a child-count change as well as on a selection change.
        let count = ctx.ui_child_count(&contents).unwrap_or(0);
        if changed || state.applied != Some((state.current, count)) {
            let keys = FILTERS[state.current].1;
            apply(&state.items, ctx, &contents, keys);
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

//! The online-pack browse screen of the Select Resource Packs screen: search box, filter chips,
//! results grid, icons and download status, driven only through a [`packs::Source`]
//! (docs/layers.md, "Platform registries"). The pack screen keeps the Add button and calls in here.
use crate::{
    hud::PACK_ICON_SIZE,
    pack_browser::Browser,
    pack_screen::icon_texture,
    screen_draw::{self as d, Draw, Measure, TextField},
    text::{Style, Text},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    sync::{Arc, LazyLock, Mutex},
};
use sunrise_hooks::packs::{self, Hit, Message, Source};

/// A card in the grid: its icon beside its name, author and description.
pub(crate) const ROW_H: i32 = 74;
/// The air around a card's contents.
pub(crate) const CARD_PAD: i32 = 6;
/// The sidebar of filters down the left.
const SIDEBAR_W: i32 = 108;
/// The bar across the top holding Back and the search box.
const TOPBAR_H: i32 = 30;
/// A filter row in the sidebar.
const FILTER_H: i32 = 15;
/// The panels' colours, dark over whatever is behind them.
const PANEL: i32 = 0xF0_14_17_22u32 as i32;
const CARD: i32 = 0xFF_1E_22_30u32 as i32;
const CARD_HOVER: i32 = 0xFF_2A_30_42u32 as i32;
const SIDEBAR_BG: i32 = 0xFF_10_12_1Bu32 as i32;
/// The Install button: green, dimmed once a pack is in.
const GREEN: i32 = 0xFF_1B_D9_6Au32 as i32;
const GREEN_HOVER: i32 = 0xFF_3B_E8_85u32 as i32;
const INSTALLED_GREEN: i32 = 0xFF_2A_5E_3Fu32 as i32;
const GREEN_EDGE: i32 = 0xFF_0E_7A_3Cu32 as i32;
/// The icon drawn at the left of a card.
pub(crate) const ICON: i32 = 34;
/// The search box and the button beside it.
pub(crate) const SEARCH_W: i32 = 200;
const SEARCH_BTN_W: i32 = 56;
/// The Back and Install buttons.
const BACK_W: i32 = 60;
pub(crate) const BACK_H: i32 = 20;
pub(crate) const INSTALL_W: i32 = 70;

fn tr(key: &str) -> Text {
    Text::translate(key, &[])
}

/// The source the pack screen browses: the first enabled registered one, else the built-in.
pub fn source() -> Option<&'static dyn Source> {
    packs::engine::source().or(Some(&*BUILTIN as &dyn Source))
}

/// Whether the Add button shows.
pub fn available() -> bool {
    source().is_some_and(|s| s.enabled())
}

/// Lets the built-in source be used by this player ([`Source::enabled`]), as the `packBrowser`
/// extra says.
pub fn allow_builtin(on: bool) {
    BUILTIN
        .seats()
        .entry(sunrise_hooks::seat())
        .or_default()
        .allowed = on;
}

/// What a click on the browse screen did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// Back: the screen closes.
    Close,
    /// Install this hit.
    Pick(usize),
    Handled,
}

/// The screen's size, which every position derives from.
#[derive(Clone, Copy, Debug)]
pub struct Geom {
    pub width: i32,
    pub height: i32,
}

impl Geom {
    pub fn back_button(self) -> (i32, i32) {
        (8, 8)
    }
    pub fn search_box(self) -> (i32, i32) {
        let total = SEARCH_W + 4 + SEARCH_BTN_W;
        let left = BACK_W + 16;
        let centred = (self.width - total) / 2;
        (centred.max(left), 5)
    }
    pub fn search_button(self) -> (i32, i32) {
        let (x, y) = self.search_box();
        (x + SEARCH_W + 4, y)
    }
    /// The Install button of card `i`: top right, beside the name.
    pub fn install_button(self, i: i32) -> (i32, i32) {
        let (cx, cy, cw, _) = self.card_box(i);
        (cx + cw - INSTALL_W - CARD_PAD, cy + CARD_PAD)
    }
    /// The content column.
    fn area(self) -> (i32, i32, i32, i32) {
        let x = SIDEBAR_W + 6;
        let top = TOPBAR_H + 6;
        let w = (self.width - x - 6).max(120);
        let h = (self.height - top - 6).max(ROW_H);
        (x, top, w, h)
    }
    /// Two columns where there is room for them, otherwise one.
    fn columns(self) -> i32 {
        let (_, _, w, _) = self.area();
        if w >= 360 { 2 } else { 1 }
    }
    /// The card at `i`: its box in the grid.
    pub fn card_box(self, i: i32) -> (i32, i32, i32, i32) {
        let (px, py, pw, _) = self.area();
        let cols = self.columns();
        let cw = (pw - 6 * (cols - 1)) / cols;
        let (cx, cy) = (i % cols, i / cols);
        (px + cx * (cw + 6), py + cy * (ROW_H + 6), cw, ROW_H)
    }
    fn filter_box(self, i: i32) -> (i32, i32, i32, i32) {
        (6, TOPBAR_H + 18 + i * FILTER_H, SIDEBAR_W - 12, FILTER_H)
    }
    /// How many cards fit the column.
    fn row_count(self) -> i32 {
        let (_, _, _, ph) = self.area();
        (((ph + 6) / (ROW_H + 6)) * self.columns()).max(0)
    }
}

/// The browse screen's state, owned by the pack screen.
#[derive(Clone, Debug)]
pub struct View {
    /// What the last search ran on.
    pub query: String,
    pub hits: Vec<Hit>,
    /// A search or a download is in flight.
    pub busy: bool,
    /// Already-translated text, shown in place of the list.
    pub status: Option<String>,
    /// The hit an install is running for.
    pub installing: Option<usize>,
    /// Hits already installed this visit, by index.
    pub installed: BTreeSet<usize>,
    /// The filters that are on, as indices into [`Source::filters`].
    pub filters: BTreeSet<usize>,
    /// The browse screen's own search box, apart from the one that filters installed packs.
    pub field: TextField,
    /// The `icons` keys this screen added, freed with it: each is `PACK_ICON_SIZE` squared in
    /// RGBA, and a search brings twenty.
    icon_keys: Vec<String>,
}

impl View {
    /// Opens on `query`, the text already in the pack filter, and searches it.
    pub fn open(m: &Measure, query: &str) -> Self {
        let mut field = TextField::new(0, 0, SEARCH_W, 18);
        field.hint = Some(tr("sunrise.pack.searchHint").with_style(&Style {
            color: Some(d::GRAY),
            italic: Some(true),
            ..Style::default()
        }));
        field.set_value(m, query);
        let mut view = Self {
            query: String::new(),
            hits: Vec::new(),
            busy: false,
            status: None,
            installing: None,
            installed: BTreeSet::new(),
            filters: BTreeSet::new(),
            field,
            icon_keys: Vec::new(),
        };
        view.query = query.to_owned();
        view.search();
        view
    }

    /// Searches for the current query and filters, unless something is already running.
    pub fn search(&mut self) {
        if let Some(source) = source() {
            self.search_with(source);
        }
    }

    pub fn search_with(&mut self, source: &dyn Source) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status = None;
        let chosen: Vec<usize> = self.filters.iter().copied().collect();
        source.search(&self.query, &chosen);
    }

    /// Runs the search on what is typed in the box (the Search button and Enter).
    pub fn submit(&mut self) {
        self.query = self.field.value.clone();
        self.search();
    }

    /// Starts installing result `index` into `dir`.
    pub fn download(&mut self, index: usize, dir: &Path, game_version: &str) {
        if let Some(source) = source() {
            self.download_with(source, index, dir, game_version);
        }
    }

    pub fn download_with(
        &mut self,
        source: &dyn Source,
        index: usize,
        dir: &Path,
        game_version: &str,
    ) {
        if self.busy || index >= self.hits.len() {
            return;
        }
        self.installing = Some(index);
        self.busy = true;
        self.status = None;
        source.download(index, dir, game_version);
    }

    /// Takes in what the source's worker finished.
    pub fn apply(&mut self, message: &Message) {
        let was_busy = self.busy;
        self.busy = false;
        match message {
            Message::Results(hits) => {
                self.hits = hits.clone();
                self.status = None;
            }
            Message::Downloaded(_) => {
                self.status = None;
                if let Some(i) = self.installing.take() {
                    self.installed.insert(i);
                }
            }
            // An icon does not end a search.
            Message::Icon(..) => self.busy = was_busy,
            Message::Failed(reason) => {
                self.status = Some(reason.clone());
                self.installing = None;
            }
            _ => {}
        }
    }

    /// Keeps a downloaded icon in `icons`, remembering it for [`Self::close`].
    pub fn keep_icon(
        &mut self,
        icons: &mut HashMap<String, Arc<[u8]>>,
        key: String,
        icon: Arc<[u8]>,
    ) {
        if icons.insert(key.clone(), icon).is_none() {
            self.icon_keys.push(key);
        }
    }

    /// Frees the icons it downloaded and tells the source the screen is gone.
    pub fn close(&mut self, icons: &mut HashMap<String, Arc<[u8]>>) {
        for key in self.icon_keys.drain(..) {
            icons.remove(&key);
        }
        icons.shrink_to_fit();
        if let Some(source) = source() {
            source.close();
        }
    }

    /// A left click on the browse screen; it is modal, so nothing beneath takes one.
    pub fn click(&mut self, m: &Measure, g: Geom, x: f64, y: f64) -> Click {
        let point = (x as i32, y as i32);
        let (bx, by) = g.back_button();
        if d::inside(point, bx, by, BACK_W, BACK_H) {
            return Click::Close;
        }
        let (qx, qy) = g.search_box();
        if d::inside(point, qx, qy, SEARCH_W, 18) {
            self.field.x = qx;
            self.field.y = qy;
            self.field.click(m, x, false, false);
            return Click::Handled;
        }
        let (sx, sy) = g.search_button();
        if d::inside(point, sx, sy, SEARCH_BTN_W, 18) {
            self.submit();
            return Click::Handled;
        }
        let count = source().map_or(0, |s| s.filters().len());
        for i in 0..count as i32 {
            let (fx, fy, fw, fh) = g.filter_box(i);
            if d::inside(point, fx, fy, fw, fh) {
                if !self.filters.insert(i as usize) {
                    self.filters.remove(&(i as usize));
                }
                // A filter is only meaningful once it is searched on.
                self.submit();
                return Click::Handled;
            }
        }
        let fits = g.row_count();
        for i in 0..(self.hits.len() as i32).min(fits) {
            let (ix, iy) = g.install_button(i);
            if d::inside(point, ix, iy, INSTALL_W, BACK_H) {
                return if self.busy {
                    Click::Handled
                } else {
                    Click::Pick(i as usize)
                };
            }
        }
        Click::Handled
    }

    pub fn draw(
        &self,
        out: &mut Vec<Draw>,
        m: &Measure,
        g: Geom,
        mouse: (i32, i32),
        now_ms: i64,
        icons: &HashMap<String, Arc<[u8]>>,
    ) {
        // Modal and opaque: vanilla's own menu background tiled over the whole screen, then a
        // wash, so none of the pack lists shows through.
        d::sprite(out, "gui/menu_background", 0, 0, g.width, g.height);
        d::fill(out, 0, 0, g.width, g.height, PANEL);
        d::fill(out, 0, 0, SIDEBAR_W, g.height, SIDEBAR_BG);
        d::fill(out, 0, 0, g.width, TOPBAR_H, SIDEBAR_BG);
        let (bx, by) = g.back_button();
        let back_hover = d::inside(mouse, bx, by, BACK_W, BACK_H);
        crate::widgets::declare_action(
            crate::widgets::Action::Back,
            [bx as f32, by as f32, BACK_W as f32, BACK_H as f32],
            true,
        );
        d::button(
            out,
            m,
            &tr("gui.back"),
            bx,
            by,
            BACK_W,
            BACK_H,
            true,
            back_hover,
            now_ms,
        );
        let (qx, qy) = g.search_box();
        let mut field = self.field.clone();
        field.x = qx;
        field.y = qy;
        field.draw(out, m, 0, None);
        if field.contains(f64::from(mouse.0), f64::from(mouse.1)) {
            crate::widgets::text_cursor(true, true);
        }
        let (sbx, sby) = g.search_button();
        d::button(
            out,
            m,
            &tr("sunrise.pack.search"),
            sbx,
            sby,
            SEARCH_BTN_W,
            18,
            !self.busy,
            d::inside(mouse, sbx, sby, SEARCH_BTN_W, 18),
            now_ms,
        );
        let heading = tr("sunrise.pack.filters").with_style(&Style {
            bold: Some(true),
            ..Style::default()
        });
        d::string_widget(out, m, &heading, 6, TOPBAR_H + 5, 0);
        for (i, key) in source().map_or(&[][..], |s| s.filters()).iter().enumerate() {
            let (fx, fy, fw, fh) = g.filter_box(i as i32);
            let on = self.filters.contains(&i);
            if d::inside(mouse, fx, fy, fw, fh) {
                d::fill(out, fx - 2, fy - 1, fx + fw + 2, fy + fh - 1, CARD_HOVER);
            }
            // A small box, filled when the filter is on.
            d::fill(out, fx, fy, fx + 7, fy + 7, 0xFF_44_4C_63u32 as i32);
            if on {
                d::fill(out, fx + 1, fy + 1, fx + 6, fy + 6, 0xFF_6C_C6_4Bu32 as i32);
            }
            let label = tr(key).with_color(if on { d::WHITE as u32 } else { d::GRAY });
            d::string_widget(out, m, &label, fx + 11, fy - 1, 0);
        }
        let (px, py, pw, ph) = g.area();
        let message = if self.busy && self.installing.is_none() {
            Some(tr("sunrise.pack.addWorking"))
        } else if let Some(status) = &self.status {
            Some(Text::translate(
                "sunrise.pack.addFailed",
                &[Text::literal(status.clone())],
            ))
        } else if self.hits.is_empty() {
            Some(tr("sunrise.pack.addNone"))
        } else {
            None
        };
        if let Some(message) = message {
            let lines = d::label_lines(
                m,
                &message.with_color(d::GRAY),
                pw - 8,
                6,
                &Style::default(),
            );
            d::label(out, &lines, px + 4, py + 20, pw - 8, true);
            return;
        }
        let fits = g.row_count();
        for (i, hit) in self.hits.iter().enumerate() {
            let i = i as i32;
            if i >= fits {
                break;
            }
            let (cx, cy, cw, ch) = g.card_box(i);
            if cy + ch > py + ph {
                break;
            }
            let over = d::inside(mouse, cx, cy, cw, ch);
            d::fill(
                out,
                cx,
                cy,
                cx + cw,
                cy + ch,
                if over { CARD_HOVER } else { CARD },
            );
            // The icon, once one has arrived: `bind_icons` swaps the texture for the atlas slot
            // it patched the PNG into.
            let texture = icon_texture(&hit.id);
            if icons.contains_key(&texture) {
                out.push(Draw::Blit {
                    texture,
                    x: cx + CARD_PAD,
                    y: cy + CARD_PAD,
                    w: ICON,
                    h: ICON,
                    u: 0.0,
                    v: 0.0,
                    src: [PACK_ICON_SIZE as i32, PACK_ICON_SIZE as i32],
                    tex: [PACK_ICON_SIZE as i32, PACK_ICON_SIZE as i32],
                    color: d::WHITE,
                });
            }
            // The name and author sit between the icon and Install; the description runs the
            // card's full width under both, so no two of them can overlap.
            let text_x = cx + CARD_PAD + ICON + CARD_PAD;
            let text_w = (cx + cw - CARD_PAD - INSTALL_W - CARD_PAD - text_x).max(20);
            let name = d::label_lines(
                m,
                &Text::literal(hit.title.clone()).with_style(&Style {
                    bold: Some(true),
                    ..Style::default()
                }),
                text_w,
                1,
                &Style::default(),
            );
            d::label(out, &name, text_x, cy + CARD_PAD + 1, text_w, false);
            if !hit.author.is_empty() {
                let author = d::label_lines(
                    m,
                    &Text::literal(hit.author.clone()).with_color(d::GRAY),
                    text_w,
                    1,
                    &Style::default(),
                );
                d::label(out, &author, text_x, cy + CARD_PAD + 12, text_w, false);
            }
            let body_w = (cw - 2 * CARD_PAD).max(20);
            let lines = d::label_lines(
                m,
                &Text::literal(hit.description.clone()).with_color(d::GRAY),
                body_w,
                2,
                &Style::default(),
            );
            // Below the icon, which is the tallest thing in the top row.
            d::label(
                out,
                &lines,
                cx + CARD_PAD,
                cy + CARD_PAD + ICON + 5,
                body_w,
                false,
            );
            let (ix, iy) = g.install_button(i);
            let done = self.installed.contains(&(i as usize));
            let running = self.installing == Some(i as usize);
            let label = if done {
                tr("sunrise.pack.installed")
            } else if running {
                tr("sunrise.pack.installing")
            } else {
                tr("sunrise.pack.install")
            };
            let hovered = d::inside(mouse, ix, iy, INSTALL_W, BACK_H);
            let face = if done {
                INSTALLED_GREEN
            } else if hovered && !self.busy {
                GREEN_HOVER
            } else {
                GREEN
            };
            d::fill(out, ix, iy, ix + INSTALL_W, iy + BACK_H, GREEN_EDGE);
            d::fill(
                out,
                ix + 1,
                iy + 1,
                ix + INSTALL_W - 1,
                iy + BACK_H - 1,
                face,
            );
            let lw = m.width(&label);
            d::string_widget(
                out,
                m,
                &label,
                ix + (INSTALL_W - lw) / 2,
                iy + (BACK_H - 8) / 2,
                0,
            );
        }
    }
}

/// What the engine keeps for one player on the built-in source.
#[derive(Default)]
struct Seat {
    allowed: bool,
    browser: Option<Browser>,
}

/// The Modrinth client in [`Browser`], used while no crate registers a [`Source`].
struct Builtin {
    seats: Mutex<BTreeMap<usize, Seat>>,
}

static BUILTIN: LazyLock<Builtin> = LazyLock::new(|| Builtin {
    seats: Mutex::new(BTreeMap::new()),
});

static FILTER_KEYS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| crate::pack_browser::FILTERS.iter().map(|f| f.0).collect());

impl Builtin {
    fn seats(&self) -> std::sync::MutexGuard<'_, BTreeMap<usize, Seat>> {
        self.seats.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Source for Builtin {
    fn enabled(&self) -> bool {
        self.seats()
            .get(&sunrise_hooks::seat())
            .is_some_and(|s| s.allowed)
    }
    fn filters(&self) -> &'static [&'static str] {
        &FILTER_KEYS
    }
    fn search(&self, query: &str, chosen: &[usize]) {
        let mut seats = self.seats();
        let browser = seats
            .entry(sunrise_hooks::seat())
            .or_default()
            .browser
            .get_or_insert_with(Browser::new);
        browser.query = query.to_owned();
        browser.filters = chosen.iter().copied().collect();
        browser.search();
    }
    fn fetch_icons(&self) {
        if let Some(b) = self
            .seats()
            .get(&sunrise_hooks::seat())
            .and_then(|s| s.browser.as_ref())
        {
            b.fetch_icons();
        }
    }
    fn download(&self, hit: usize, dir: &Path, game_version: &str) {
        if let Some(b) = self
            .seats()
            .get_mut(&sunrise_hooks::seat())
            .and_then(|s| s.browser.as_mut())
        {
            b.download(hit, dir, game_version);
        }
    }
    fn poll(&self) -> Option<Message> {
        self.seats()
            .get_mut(&sunrise_hooks::seat())?
            .browser
            .as_mut()?
            .poll()
    }
    fn close(&self) {
        if let Some(seat) = self.seats().get_mut(&sunrise_hooks::seat()) {
            seat.browser = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Fake {
        searches: AtomicUsize,
        downloads: AtomicUsize,
    }

    impl Source for Fake {
        fn enabled(&self) -> bool {
            true
        }
        fn filters(&self) -> &'static [&'static str] {
            &["a", "b"]
        }
        fn search(&self, _: &str, _: &[usize]) {
            self.searches.fetch_add(1, Ordering::Relaxed);
        }
        fn fetch_icons(&self) {}
        fn download(&self, _: usize, _: &Path, _: &str) {
            self.downloads.fetch_add(1, Ordering::Relaxed);
        }
        fn poll(&self) -> Option<Message> {
            None
        }
        fn close(&self) {}
    }

    fn hit() -> Hit {
        Hit {
            id: "p".into(),
            title: "P".into(),
            author: String::new(),
            description: String::new(),
            downloads: 0,
            icon_url: String::new(),
        }
    }

    fn view() -> View {
        View {
            query: String::new(),
            hits: Vec::new(),
            busy: false,
            status: None,
            installing: None,
            installed: BTreeSet::new(),
            filters: BTreeSet::new(),
            field: TextField::new(0, 0, SEARCH_W, 18),
            icon_keys: Vec::new(),
        }
    }

    #[test]
    fn one_search_runs_at_a_time_and_results_free_the_view() {
        let fake = Fake::default();
        let mut v = view();
        v.search_with(&fake);
        v.search_with(&fake);
        assert_eq!(fake.searches.load(Ordering::Relaxed), 1);
        v.apply(&Message::Results(vec![hit()]));
        assert!(!v.busy);
        assert_eq!(v.hits.len(), 1);
    }

    #[test]
    fn a_download_marks_its_card_installed_and_a_failure_shows_its_reason() {
        let fake = Fake::default();
        let mut v = view();
        v.apply(&Message::Results(vec![hit()]));
        v.download_with(&fake, 0, Path::new("."), "26.3");
        v.download_with(&fake, 0, Path::new("."), "26.3");
        assert_eq!(fake.downloads.load(Ordering::Relaxed), 1);
        assert_eq!(v.installing, Some(0));
        v.apply(&Message::Downloaded("p.zip".into()));
        assert!(v.installed.contains(&0) && !v.busy && v.installing.is_none());
        v.download_with(&fake, 0, Path::new("."), "26.3");
        v.apply(&Message::Failed("offline".into()));
        assert_eq!(v.status.as_deref(), Some("offline"));
        assert!(v.installing.is_none());
    }

    #[test]
    fn a_cards_parts_do_not_overlap_each_other() {
        // Both a wide window (two columns) and a narrow one (one column).
        for (w, h) in [(480, 320), (260, 320)] {
            let g = Geom {
                width: w,
                height: h,
            };
            let (cx, cy, cw, ch) = g.card_box(0);
            let (ix, iy) = g.install_button(0);
            assert!(ix >= cx && ix + INSTALL_W <= cx + cw, "install escapes");
            assert!(iy >= cy && iy + BACK_H <= cy + ch, "install escapes");
            let text_x = cx + CARD_PAD + ICON + CARD_PAD;
            let text_w = (cx + cw - CARD_PAD - INSTALL_W - CARD_PAD - text_x).max(20);
            assert!(
                text_x + text_w <= ix || text_w == 20,
                "name runs under Install"
            );
            assert!(
                cy + CARD_PAD + ICON + 5 + 18 <= cy + ch,
                "description overflows"
            );
        }
    }

    #[test]
    fn the_built_in_source_is_enabled_only_when_allowed() {
        allow_builtin(false);
        assert!(!available());
        allow_builtin(true);
        assert!(available());
        allow_builtin(false);
    }
}

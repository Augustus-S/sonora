//! The Theme section of Appearance: one card per theme on a rail that scrolls sideways, each
//! card a flat miniature of the Sonora window painted in that theme.

use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, Hsla, Pixels, SharedString, div, px, svg};
use i18n::t;
use state::ThemeFamily;
use ui::{
    ActiveTheme as _, Button, Look, Menu, MenuItem, OptionCard, Picker, Popover, Rail, Theme,
    ThemeKind,
};

use super::SettingsView;

/// How wide a theme card stands.
const CARD: Pixels = px(150.);
/// How tall a theme card's picture stands, its border included.
const THUMB: Pixels = px(96.);
/// How thick the card's border is, which the miniature sits inside.
const EDGE: Pixels = px(1.);
/// The miniature window inside the card's border, in whole pixels.
const WIDTH: f32 = 148.;
const HEIGHT: f32 = 94.;
/// How many variants of a family its card shows side by side at most.
const SLICES: usize = 4;
/// The popover key of the variant menu, which one card at a time owns.
const VARIANTS: &str = "theme-variants";
/// The side of a variant's swatch in the menu, and of the dot of its accent inside it.
const SWATCH: Pixels = px(16.);
const SWATCH_DOT: Pixels = px(6.);
/// How round a variant's swatch in the menu is.
const SWATCH_ROUND: Pixels = px(4.);
/// How large the open folder glyph draws on its card.
const GLYPH: Pixels = px(18.);
/// How round the player bar's cover grows at most, so it stays a square of art.
const COVER_ROUND: Pixels = px(2.);
/// How much of the theme's corner radius the miniature's marks keep.
const RADIUS_SCALE: f32 = 0.25;

/// The miniature's frame, in pixels from its top left corner: the sidebar's width, the row of
/// the line under the title bar, and the row of the line over the player bar. The sidebar runs
/// the full height above the player bar, so it alone carries the top left corner.
const SIDEBAR: f32 = 38.;
const TITLE: f32 = 9.;
const PLAYER: f32 = 76.;
/// The columns a cut between two palettes may run down: the sidebar's edge and the middle of
/// each gap between two tiles, so a cut never splits a mark.
const SEAMS: [f32; 3] = [SIDEBAR, 75., 110.];
/// The active entry's pill in the sidebar and the label on it, then the other entries' labels,
/// as left, top, width and height.
const PILL: [f32; 4] = [4., 13., 30., 9.];
const PILL_LABEL: [f32; 4] = [8., 16., 16., 3.];
const ENTRIES: [[f32; 4]; 3] = [[8., 28., 20., 3.], [8., 36., 14., 3.], [8., 44., 18., 3.]];
/// The content's heading, and the row of tiles under it by their left edges, the first one in
/// the accent.
const HEADING: [f32; 4] = [44., 16., 28., 5.];
const TILES: [f32; 3] = [44., 79., 114.];
const TILE_TOP: f32 = 28.;
const TILE: f32 = 28.;
/// The player bar's cover, its play button, and its progress line with the part played.
const COVER: [f32; 4] = [8., 81., 9., 9.];
const PLAY: [f32; 4] = [82., 82., 6., 6.];
const TRACK: [f32; 4] = [94., 84., 42., 2.];
const PLAYED: f32 = 16.;

/// One card of the rail: a built-in theme or a family of theme files.
enum Entry {
    Kind(ThemeKind),
    Family(ThemeFamily),
}

impl SettingsView {
    /// The theme cards on one rail: System, Dark and Light, then every family of theme files,
    /// then the card that opens the themes folder. While the adaptive theme is on only the
    /// neutral themes can be chosen, since the tint is laid over those. A saved theme whose file
    /// is gone keeps a card of its own, chosen and disabled, so the page says what became of it.
    pub(super) fn theme_cards(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let (selected, chosen_family, adaptive, families) = {
            let settings = self.settings.read(cx);
            (
                settings.theme().to_owned(),
                settings
                    .theme_variant()
                    .map(|(family, _)| family.id.clone()),
                settings.adaptive_theme(),
                settings.theme_families().to_vec(),
            )
        };
        let built_in = ThemeKind::ALL.iter().any(|kind| kind.id() == selected);

        let mut cards = ThemeKind::ALL
            .into_iter()
            .map(Entry::Kind)
            .chain(families.into_iter().map(Entry::Family))
            .map(|entry| {
                let chosen = match &entry {
                    Entry::Kind(kind) => kind.id() == selected,
                    Entry::Family(family) => chosen_family.as_ref() == Some(&family.id),
                };
                self.theme_card(entry, chosen, adaptive, cx)
            })
            .collect::<Vec<_>>();

        if !built_in && chosen_family.is_none() {
            cards.push(
                OptionCard::new("unavailable-theme")
                    .w(CARD)
                    .thumb_height(THUMB)
                    .label(t!("theme-unavailable", name = selected.clone()))
                    .selected(true)
                    .disabled(true)
                    .into_any_element(),
            );
        }

        let settings = self.settings.clone();
        cards.push(
            OptionCard::new("open-theme-folder")
                .w(CARD)
                .thumb_height(THUMB)
                .dashed()
                .thumb(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path(icons::path("icons/folder-plus.svg"))
                                .size(GLYPH)
                                .text_color(theme.muted_foreground),
                        ),
                )
                .label(t!("settings-theme-folder"))
                .on_click(move |_, _, cx| {
                    let path =
                        settings.update(cx, |settings, _| settings.ensure_themes_directory());
                    if let Err(error) = open::that_detached(&path) {
                        log::warn!("settings: cannot open {}: {error}", path.display());
                    }
                })
                .into_any_element(),
        );

        // the ring around the chosen card stands outside it, so the rail leaves it room
        let room = theme.metrics.pad;
        Rail::new("theme-cards")
            .py(room)
            .arrows_at(room + THUMB / 2.)
            .children(cards)
            .into_any_element()
    }

    /// One card: its miniature and its name, and for a family of several variants a button
    /// under the name that says which variant a click picks and lists them all.
    fn theme_card(
        &self,
        entry: Entry,
        chosen: bool,
        adaptive: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = self.look(cx);
        let radius = cx.theme().radius;
        let divider = cx.theme().border;

        match entry {
            Entry::Kind(kind) => {
                let palettes = match kind {
                    ThemeKind::System => vec![Theme::dark(), Theme::light()],
                    kind => vec![Theme::for_kind(kind)],
                };
                OptionCard::new(kind.id())
                    .w(CARD)
                    .thumb_height(THUMB)
                    .thumb(sliced(&palettes, divider, radius))
                    .label(kind.label())
                    .selected(chosen)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let overrides = this.settings.update(cx, |settings, cx| {
                            settings.set_theme(kind.id(), cx);
                            settings.theme_overrides()
                        });
                        Theme::fade(Look { kind, ..look }, &overrides, cx);
                        cx.notify();
                    }))
                    .into_any_element()
            }
            Entry::Family(family) => {
                let palettes = family
                    .variants
                    .iter()
                    .take(SLICES)
                    .map(|variant| variant.palette())
                    .collect::<Vec<_>>();
                let several = family.variants.len() > 1;
                // the variant on show while the family is chosen, else the one a click would pick
                let settings = self.settings.read(cx);
                let picked = match chosen {
                    true => settings.theme_variant().map(|(_, variant)| variant),
                    false => settings.family_variant(&family.id, cx),
                }
                .map(|variant| (variant.id.clone(), variant.name.clone()));
                let menu = picked
                    .filter(|_| several && !adaptive)
                    .map(|(id, name)| self.variant_menu(&family, &id, name, cx));
                let id = family.id.clone();

                OptionCard::new(SharedString::from(format!("theme-family:{}", family.id)))
                    .w(CARD)
                    .thumb_height(THUMB)
                    .thumb(sliced(&palettes, divider, radius))
                    .label(family.name.clone())
                    .when_some(menu, |card, menu| card.accessory(menu))
                    .selected(chosen)
                    .disabled(adaptive)
                    .when(adaptive, |card| card.tooltip("settings-theme-unavailable"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let overrides = this.settings.update(cx, |settings, cx| {
                            settings.set_theme_family(&id, cx);
                            settings.theme_overrides()
                        });
                        Theme::fade(
                            Look {
                                kind: ThemeKind::Dark,
                                ..look
                            },
                            &overrides,
                            cx,
                        );
                        cx.notify();
                    }))
                    .into_any_element()
            }
        }
    }

    /// The button under a family's name that shows `name`, the variant `picked`, with a chevron,
    /// and the menu of every variant it opens with that one checked. Only the family whose menu
    /// opened last owns the popover, so the other cards draw a plain button that hands it to
    /// theirs.
    fn variant_menu(
        &self,
        family: &ThemeFamily,
        picked: &str,
        name: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let button = Button::new(SharedString::from(format!("theme-variants:{}", family.id)))
            .small()
            .label(name)
            .trailing("icons/chevron-down.svg")
            .tint(theme.muted_foreground)
            .tooltip("settings-theme-variants");

        if self.theme_variants.as_deref() != Some(family.id.as_str()) {
            let id = family.id.clone();
            return button
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.theme_variants = Some(id.clone());
                    this.popovers.open(VARIANTS);
                    cx.notify();
                }))
                .into_any_element();
        }

        let look = self.look(cx);
        let items = family.variants.iter().map(|variant| {
            let id = variant.id.clone();
            MenuItem::new(
                SharedString::from(format!("theme-variant:{}", variant.id)),
                variant.name.clone(),
            )
            .leading(swatch(&variant.palette(), theme.border))
            .checked(picked == variant.id)
            .on_click(cx.listener(move |this, _, _, cx| {
                let overrides = this.settings.update(cx, |settings, cx| {
                    settings.set_theme(id.clone(), cx);
                    settings.theme_overrides()
                });
                Theme::fade(
                    Look {
                        kind: ThemeKind::Dark,
                        ..look
                    },
                    &overrides,
                    cx,
                );
                this.popovers.close();
                cx.notify();
            }))
        });
        let menu = Menu::new("theme-variants-menu")
            .w(Picker::NARROW)
            .items(items)
            .top(theme.metrics.control_small + theme.metrics.pad / 2.)
            .left_0();

        Popover::new(VARIANTS, self.popovers.clone())
            .button(button)
            .menu(menu)
            .commands()
            .into_any_element()
    }
}

/// The miniature cut into one vertical slice per palette, left to right, each slice the same
/// window painted in its own palette so the layout runs on across the cuts. The cuts run down
/// `SEAMS`, and a `divider` line marks each. `radius` is the card's, whose inside the
/// miniature's outer corners follow.
fn sliced(palettes: &[Theme], divider: Hsla, radius: Pixels) -> Div {
    let count = palettes.len().max(1);
    let cuts = (1..count).map(|index| {
        let even = WIDTH * index as f32 / count as f32;
        SEAMS
            .into_iter()
            .min_by(|a, b| (a - even).abs().total_cmp(&(b - even).abs()))
            .unwrap_or(even)
    });
    let edges: Vec<f32> = std::iter::once(0.)
        .chain(cuts)
        .chain(std::iter::once(WIDTH))
        .collect();
    let mark = (radius * RADIUS_SCALE).floor();
    // the card does not clip to its corners, so the miniature rounds its own inside the border
    let outer = f32::from((radius - EDGE).max(Pixels::ZERO)).min(SIDEBAR / 2.);

    div()
        .relative()
        .size_full()
        .children(
            palettes
                .iter()
                .zip(edges.windows(2))
                .map(|(palette, edge)| {
                    let (left, right) = (edge[0], edge[1]);
                    rect([left, 0., right - left, HEIGHT])
                        .overflow_hidden()
                        .child(miniature(palette, outer, mark).left(px(-left)))
                }),
        )
        .children(
            edges[1..edges.len() - 1]
                .iter()
                .map(|cut| rect([*cut, 0., 1., HEIGHT]).bg(divider)),
        )
}

/// The Sonora window in `palette`'s colours as flat shapes, `WIDTH` by `HEIGHT` pixels: the
/// sidebar with its active pill and three entries, the title bar's line over the content, a
/// heading over a row of tiles, and the player bar with a cover, the play button and the
/// progress line. Every shape sits on whole pixels so its edges stay sharp. The shapes that
/// meet the outer corners round them by `outer`, and `mark` rounds the small ones.
fn miniature(palette: &Theme, outer: f32, mark: Pixels) -> Div {
    let outer_px = px(outer);
    let title_end = WIDTH - inset(TITLE, outer);
    let player_inset = inset(PLAYER, outer);

    div()
        .absolute()
        .top_0()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .child(
            rect([0., HEIGHT / 2., WIDTH, HEIGHT / 2.])
                .rounded_b(outer_px)
                .bg(palette.secondary),
        )
        .child(
            rect([0., 0., SIDEBAR, PLAYER])
                .rounded_tl(outer_px)
                .bg(palette.sidebar),
        )
        .child(
            rect([SIDEBAR + 1., 0., WIDTH - SIDEBAR - 1., PLAYER])
                .rounded_tr(outer_px)
                .bg(palette.background),
        )
        .child(rect([SIDEBAR, 0., 1., PLAYER]).bg(palette.sidebar_border))
        .child(
            rect([SIDEBAR + 1., TITLE, title_end - SIDEBAR - 1., 1.]).bg(palette.title_bar_border),
        )
        .child(
            rect([player_inset, PLAYER, WIDTH - 2. * player_inset, 1.]).bg(palette.sidebar_border),
        )
        .child(rect(PILL).rounded(mark).bg(palette.sidebar_accent))
        .child(rect(PILL_LABEL).rounded(mark).bg(palette.foreground))
        .children(ENTRIES.map(|entry| rect(entry).rounded(mark).bg(palette.muted_foreground)))
        .child(rect(HEADING).rounded(mark).bg(palette.foreground))
        .children(TILES.into_iter().enumerate().map(|(index, left)| {
            let fill = match index {
                0 => palette.primary,
                _ => palette.muted,
            };
            rect([left, TILE_TOP, TILE, TILE]).rounded(mark).bg(fill)
        }))
        .child(rect(COVER).rounded(mark.min(COVER_ROUND)).bg(palette.muted))
        .child(rect(PLAY).rounded_full().bg(palette.primary))
        .child(rect(TRACK).rounded_full().bg(palette.muted))
        .child(
            rect([TRACK[0], TRACK[1], PLAYED, TRACK[3]])
                .rounded_full()
                .bg(palette.progress_bar),
        )
}

/// How far in from either side the outline of a window rounded by `outer` runs on the pixel row
/// at `y`, rounded up to a whole pixel, so a line drawn there stays inside the corners.
fn inset(y: f32, outer: f32) -> f32 {
    let middle = y + 0.5;
    let reach = match middle {
        middle if middle < outer => outer - middle,
        middle if middle > HEIGHT - outer => middle - (HEIGHT - outer),
        _ => return 0.,
    };
    (outer - (outer * outer - reach * reach).max(0.).sqrt()).ceil()
}

/// A variant's swatch in the menu: a square of its page colour around a dot of its accent.
fn swatch(palette: &Theme, edge: Hsla) -> Div {
    div()
        .size(SWATCH)
        .flex()
        .items_center()
        .justify_center()
        .rounded(SWATCH_ROUND)
        .border_1()
        .border_color(edge)
        .bg(palette.background)
        .child(div().size(SWATCH_DOT).rounded_full().bg(palette.primary))
}

/// A box in the miniature from its left, top, width and height in pixels.
fn rect([x, y, width, height]: [f32; 4]) -> Div {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(width))
        .h(px(height))
}

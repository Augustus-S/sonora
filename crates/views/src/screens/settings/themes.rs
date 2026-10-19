//! The Theme section of Appearance: one card per theme on a rail that scrolls sideways, each
//! card a miniature of the Sonora window painted in that theme.

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Div, Hsla, Pixels, SharedString, div, linear_color_stop, linear_gradient,
    px, svg,
};
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
/// The miniature window inside the card's 1px border, in whole pixels.
const WIDTH: f32 = 148.;
const HEIGHT: f32 = 94.;
/// How many variants of a family its card shows side by side at most.
const SLICES: usize = 4;
/// The popover key of the variant menu, which one card at a time owns.
const VARIANTS: &str = "theme-variants";
/// The side of the button that opens a family's variants.
const DOTS: Pixels = px(20.);
/// How far that button sits from the picture's top right corner.
const DOTS_INSET: Pixels = px(4.);
/// The side of a variant's swatch in the menu, and of the dot of its accent inside it.
const SWATCH: Pixels = px(16.);
const SWATCH_DOT: Pixels = px(6.);
/// How large the open folder glyph draws on its card.
const GLYPH: Pixels = px(18.);
/// How much of the theme's corner radius the miniature keeps.
const RADIUS_SCALE: f32 = 0.25;

/// The miniature's window, in pixels from its top left corner. The title bar runs across the
/// top above its border line, the sidebar down the left beside its own, and the player bar
/// along the bottom below a third.
const TITLE: f32 = 7.;
const SIDEBAR: f32 = 36.;
const PLAYER: f32 = 80.;
/// Where the sidebar's rows start and how far apart they stand.
const NAV_TOP: f32 = 11.;
const NAV_PITCH: f32 = 8.;
/// How long each sidebar entry's label runs, the first one active.
const NAV: [f32; 5] = [13., 12., 16., 14., 11.];
/// Where the pinned list starts under its eyebrow.
const PINNED: f32 = 56.;
/// The left edge of the content and the side of an album and the gap between two.
const CONTENT: f32 = 45.;
const ALBUM: f32 = 20.;
const ALBUM_GAP: f32 = 5.;
/// The top of each shelf of albums.
const SHELVES: [f32; 2] = [22., 63.];
/// How long the heading over each shelf runs.
const HEADINGS: [f32; 2] = [28., 22.];
/// How strongly each album's art takes the theme's accent, shelf by shelf.
const ART: [[f32; 4]; 2] = [[0.95, 0.5, 0.75, 0.32], [0.42, 0.85, 0.28, 0.62]];
/// How much of its strength an album's accent keeps at the far corner.
const FADED_ART: f32 = 0.35;
/// The angles the accent fades across the albums at, one after another along a shelf.
const ANGLES: [f32; 4] = [135., 160., 115., 150.];
/// How round the miniature's lines and marks are, and how round at most its pill, its albums
/// and its covers grow with the theme's radius.
const NIB: Pixels = px(1.);
const PILL_ROUND: Pixels = px(4.);
const ALBUM_ROUND: Pixels = px(3.);
const COVER_ROUND: Pixels = px(2.);
/// How round a variant's swatch in the menu is.
const SWATCH_ROUND: Pixels = px(4.);

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

    /// One card: its miniature, its name, and for a family of several variants the name of the
    /// one a click picks and the button that lists them all.
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
                let dots = several
                    .then(|| self.variant_menu(&family, picked.as_ref().map(|(id, _)| id), cx))
                    .filter(|_| !adaptive);
                let id = family.id.clone();

                OptionCard::new(SharedString::from(format!("theme-family:{}", family.id)))
                    .w(CARD)
                    .thumb_height(THUMB)
                    .thumb(
                        sliced(&palettes, divider, radius)
                            .children(dots)
                            .into_any_element(),
                    )
                    .label(family.name.clone())
                    .when_some(picked.filter(|_| several), |card, (_, name)| {
                        card.detail(name)
                    })
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

    /// The three dots at the top right of a family's picture and the menu of its variants they
    /// open, with `picked` checked. Only the family whose menu opened last owns the popover, so
    /// the other cards draw a plain button that hands it to theirs.
    fn variant_menu(
        &self,
        family: &ThemeFamily,
        picked: Option<&String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.theme();
        let button = Button::new(SharedString::from(format!("theme-variants:{}", family.id)))
            .icon("icons/ellipsis.svg")
            .small()
            .secondary()
            .tooltip("settings-theme-variants")
            .size(DOTS)
            .px_0()
            .shadow_sm();
        let place = |this: Div| this.absolute().top(DOTS_INSET).right(DOTS_INSET);

        if self.theme_variants.as_deref() != Some(family.id.as_str()) {
            let id = family.id.clone();
            return place(div())
                .child(button.on_click(cx.listener(move |this, _, _, cx| {
                    this.theme_variants = Some(id.clone());
                    this.popovers.open(VARIANTS);
                    cx.notify();
                })))
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
            .checked(picked == Some(&variant.id))
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
            .top(DOTS + theme.metrics.pad / 2.)
            .right_0();

        place(div())
            .child(
                Popover::new(VARIANTS, self.popovers.clone())
                    .button(button)
                    .menu(menu)
                    .commands(),
            )
            .into_any_element()
    }
}

/// The miniature cut into one vertical slice per palette, left to right, each slice the same
/// window painted in its own palette so the layout runs on across the cuts. A `divider` line
/// marks each cut.
fn sliced(palettes: &[Theme], divider: Hsla, radius: Pixels) -> Div {
    let count = palettes.len().max(1);
    let edge = |index: usize| (WIDTH * index as f32 / count as f32).round();
    let corner = (radius * RADIUS_SCALE).floor();

    div()
        .relative()
        .size_full()
        .children(palettes.iter().enumerate().map(|(index, palette)| {
            let (left, right) = (edge(index), edge(index + 1));
            rect(left, 0., right - left, HEIGHT)
                .overflow_hidden()
                .child(miniature(palette, corner).left(px(-left)))
        }))
        .children((1..count).map(|index| rect(edge(index), 0., 1., HEIGHT).bg(divider)))
}

/// The Sonora window in `palette`'s colours, `WIDTH` by `HEIGHT` pixels: the title bar with its
/// window controls, the sidebar with its entries and the active one on a pill, two shelves of
/// albums under their headings, and the player bar with a cover, the play button and the
/// progress line. Every part sits on whole pixels so its edges stay sharp. `corner` is the
/// theme's radius scaled to the miniature.
fn miniature(palette: &Theme, corner: Pixels) -> Div {
    let player_top = PLAYER + 1.;
    let body = PLAYER - TITLE - 1.;

    div()
        .absolute()
        .top_0()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .bg(palette.background)
        .child(
            rect(0., TITLE + 1., SIDEBAR, body)
                .bg(palette.sidebar)
                .child(
                    rect(3., NAV_TOP - TITLE - 1., SIDEBAR - 6., NAV_PITCH)
                        .rounded(corner.min(PILL_ROUND))
                        .bg(palette.sidebar_accent),
                ),
        )
        .children(NAV.iter().enumerate().flat_map(|(index, width)| {
            let top = NAV_TOP + NAV_PITCH * index as f32;
            let color = match index {
                0 => palette.foreground,
                _ => palette.muted_foreground,
            };
            [
                rect(6., top + 2., 4., 4.).rounded(NIB).bg(color),
                line(12., top + 3., *width, color),
            ]
        }))
        .child(rect(6., PINNED, 10., 1.).bg(palette.muted_foreground))
        .children(
            [0.8, 0.45]
                .into_iter()
                .enumerate()
                .flat_map(|(index, strength)| {
                    let top = PINNED + 4. + NAV_PITCH * index as f32;
                    [
                        art(palette, 6., top, 6., strength, index).rounded(corner.min(COVER_ROUND)),
                        line(14., top + 2., 14. - 4. * index as f32, palette.foreground),
                    ]
                }),
        )
        .child(rect(SIDEBAR, TITLE + 1., 1., body).bg(palette.sidebar_border))
        .children(SHELVES.into_iter().zip(HEADINGS).zip(ART).flat_map(
            |((top, heading), strengths)| shelf(palette, top, heading, strengths, corner),
        ))
        .child(
            rect(5., 2., 3., 3.)
                .rounded(NIB)
                .border_1()
                .border_color(palette.muted_foreground),
        )
        .children([130., 135., 140.].map(|x| {
            rect(x, 2., 3., 3.)
                .rounded_full()
                .bg(palette.muted_foreground)
        }))
        .child(rect(0., TITLE, WIDTH, 1.).bg(palette.title_bar_border))
        .child(rect(0., PLAYER, WIDTH, 1.).bg(palette.sidebar_border))
        .child(rect(0., player_top, WIDTH, HEIGHT - player_top).bg(palette.background))
        .child(art(palette, 5., 84., 7., 0.9, 0).rounded(corner.min(COVER_ROUND)))
        .child(line(15., 85., 16., palette.foreground))
        .child(line(15., 88., 10., palette.muted_foreground))
        .children([62., 84.].map(|x| {
            rect(x, 84., 2., 2.)
                .rounded_full()
                .bg(palette.muted_foreground)
        }))
        .child(rect(71., 82., 6., 6.).rounded_full().bg(palette.primary))
        .child(line(50., 90., 48., palette.muted))
        .child(line(50., 90., 20., palette.progress_bar))
        .child(line(118., 86., 20., palette.muted))
        .child(line(118., 86., 13., palette.foreground))
}

/// One shelf of the miniature's content: its heading, then four albums from `top` down, each
/// with its title and artist under it. A shelf running past the player bar is covered by it.
fn shelf(
    palette: &Theme,
    top: f32,
    heading: f32,
    strengths: [f32; 4],
    corner: Pixels,
) -> impl Iterator<Item = Div> {
    let heading = rect(CONTENT, top - 8., heading, 4.)
        .rounded(NIB)
        .bg(palette.foreground);
    let title = palette.foreground.opacity(0.85);
    let artist = palette.muted_foreground;
    let albums = strengths
        .into_iter()
        .enumerate()
        .flat_map(move |(index, strength)| {
            let left = CONTENT + (ALBUM + ALBUM_GAP) * index as f32;
            [
                art(palette, left, top, ALBUM, strength, index).rounded(corner.min(ALBUM_ROUND)),
                line(left, top + ALBUM + 3., 14., title),
                line(left, top + ALBUM + 7., 10., artist),
            ]
        });
    std::iter::once(heading).chain(albums)
}

/// A square of cover art `size` pixels wide at `x`, `y`, the theme's accent at `strength`
/// fading across it at the angle the `place`th cover of a row takes.
fn art(palette: &Theme, x: f32, y: f32, size: f32, strength: f32, place: usize) -> Div {
    rect(x, y, size, size).bg(linear_gradient(
        ANGLES[place % ANGLES.len()],
        linear_color_stop(palette.primary.opacity(strength), 0.),
        linear_color_stop(palette.primary.opacity(strength * FADED_ART), 1.),
    ))
}

/// A line of text in the miniature, two pixels tall.
fn line(x: f32, y: f32, width: f32, color: Hsla) -> Div {
    rect(x, y, width, 2.).rounded(NIB).bg(color)
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

/// A box at `x`, `y` in the miniature, `width` by `height` pixels.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Div {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(width))
        .h(px(height))
}

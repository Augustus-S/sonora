pub(crate) mod fullscreen;
pub(crate) mod settings;
pub(crate) mod workspace;

use gpui::{AnyView, App, KeyContext};

use crate::chrome::TitleBarOptions;

pub(crate) trait Shell {
    fn title_bar(&self, content: Option<AnyView>, cx: &App) -> TitleBarOptions;
}

/// The key context a shell sets on its root, built from the bare names. A shell renders every
/// frame an animation in it runs, and parsing the names each time would copy them every frame.
pub(crate) fn shell_context(names: &[&'static str]) -> KeyContext {
    let mut context = KeyContext::default();
    for name in names {
        context.add(*name);
    }
    context
}

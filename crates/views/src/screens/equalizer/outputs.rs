use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div};
use i18n::t;
use music::output::OutputId;
use state::equalizer_profile_label;
use ui::{InfoCard, MenuItem, Picker, Separator};

use super::{EqualizerView, blurb, entry};

const THIS_OUTPUT: &str = "equalizer-output-this-picker";
const OTHER_OUTPUTS: &str = "equalizer-output-others-picker";

impl EqualizerView {
    /// The devices' card: the profile bound to the output played on now, and the profile every
    /// output with none of its own plays. Left out while nothing says which output plays.
    pub(super) fn outputs(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let outputs = self.outputs.read(cx);
        let output = outputs.binding_key()?;
        let port = outputs.sink().and_then(|sink| sink.port.clone());
        let settings = self.settings.read(cx);
        let names: Vec<SharedString> = settings
            .equalizer_profiles()
            .map(equalizer_profile_label)
            .collect();
        let bound = settings.equalizer_profile_bound_to(&output.id);
        let others = settings.equalizer_other_outputs();
        let shown = |index: Option<usize>, none: SharedString| {
            index
                .and_then(|index| names.get(index).cloned())
                .unwrap_or(none)
        };

        let this = Picker::new(
            THIS_OUTPUT,
            &self.popovers,
            shown(bound, t!("equalizer-output-unbound")),
        )
        .width(Picker::NARROW)
        .items(self.choices(
            THIS_OUTPUT,
            &names,
            bound,
            t!("equalizer-output-unbound"),
            {
                let output = output.clone();
                move |this, index, cx| this.bind(&output, index, cx)
            },
            cx,
        ));
        let other = Picker::new(
            OTHER_OUTPUTS,
            &self.popovers,
            shown(others, t!("equalizer-output-keep")),
        )
        .width(Picker::NARROW)
        .items(self.choices(
            OTHER_OUTPUTS,
            &names,
            others,
            t!("equalizer-output-keep"),
            |this, index, cx| {
                this.equalizer
                    .update(cx, |equalizer, cx| equalizer.set_other_outputs(index, cx))
            },
            cx,
        ));

        Some(
            InfoCard::new(t!("equalizer-devices"))
                .child(blurb(t!("equalizer-devices-detail"), None, cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .my_neg_3()
                        .child(entry(
                            t!("equalizer-output-this"),
                            output_name(&output.name, port.as_deref()),
                            this,
                            cx,
                        ))
                        .child(Separator::horizontal())
                        .child(entry(
                            t!("equalizer-output-others"),
                            t!("equalizer-output-others-detail"),
                            other,
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }

    /// No profile, then every profile, the chosen one marked.
    fn choices(
        &self,
        id: &'static str,
        names: &[SharedString],
        chosen: Option<usize>,
        none: SharedString,
        pick: impl Fn(&mut Self, Option<usize>, &mut Context<Self>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> Vec<MenuItem> {
        let options = std::iter::once((None, none))
            .chain(names.iter().cloned().enumerate().map(|(i, n)| (Some(i), n)));
        options
            .map(|(index, label)| {
                let pick = pick.clone();
                MenuItem::new((id, index.map_or(0, |index| index + 1)), label)
                    .selected(index == chosen)
                    .on_click(cx.listener(move |this, _, _, cx| pick(this, index, cx)))
            })
            .collect()
    }

    fn bind(&mut self, output: &OutputId, index: Option<usize>, cx: &mut Context<Self>) {
        self.equalizer
            .update(cx, |equalizer, cx| equalizer.bind_output(output, index, cx));
    }
}

/// An output's name, with the port that plays when it has more than one.
pub(super) fn output_name(name: &str, port: Option<&str>) -> SharedString {
    match port {
        Some(port) => t!(
            "equalizer-output-port",
            device = name.to_owned(),
            port = port.to_owned()
        ),
        None => name.to_owned().into(),
    }
}

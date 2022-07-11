use crate::ui::{
    component::{Child, Component, Event, EventCtx, Pad},
    geometry::Rect,
};

use super::{common::ChoiceItem, theme, ButtonController, ButtonControllerMsg, ButtonPos};
use heapless::Vec;

pub enum ChoicePageMsg {
    Choice(u8),
    LeftMost,
    RightMost,
}

const MIDDLE_ROW: i32 = 72;

/// General component displaying a set of items on the screen
/// and allowing the user to select one of them.
///
/// To be used by other more specific components that will
/// supply a set of `ChoiceItem`s and will receive back
/// the index of the selected choice.
///
/// Each `ChoiceItem` is responsible for setting the screen -
/// choosing the button text, their duration, text displayed
/// on screen etc.
pub struct ChoicePage<T, const N: usize> {
    choices: Vec<T, N>,
    pad: Pad,
    buttons: Child<ButtonController<&'static str>>,
    page_counter: u8,
}

impl<T, const N: usize> ChoicePage<T, N>
where
    T: ChoiceItem,
{
    pub fn new(choices: Vec<T, N>) -> Self {
        let initial_btn_layout = choices[0].btn_layout();

        Self {
            choices,
            pad: Pad::with_background(theme::BG),
            buttons: Child::new(ButtonController::new(initial_btn_layout)),
            page_counter: 0,
        }
    }

    /// Resetting the component, which enables reusing the same instance
    /// for multiple choice categories.
    ///
    /// NOTE: from the client point of view, it would also be an option to
    /// always create a new instance with fresh setup, but I could not manage to
    /// properly clean up the previous instance - it would still be shown on
    /// screen and colliding with the new one.
    pub fn reset(&mut self, ctx: &mut EventCtx, new_choices: Vec<T, N>, reset_page_counter: bool) {
        self.choices = new_choices;
        if reset_page_counter {
            self.page_counter = 0;
        }
        self.update(ctx);
    }

    /// Navigating to the chosen page index.
    pub fn set_page_counter(&mut self, ctx: &mut EventCtx, page_counter: u8) {
        self.page_counter = page_counter;
        self.update(ctx);
    }

    /// Display current, previous and next choice according to
    /// the current ChoiceItem.
    fn paint_choices(&mut self) {
        // MIDDLE section above buttons
        // Performing the appropriate `paint_XXX()` for the main choice
        // and two adjacent choices when present
        self.show_current_choice();
        if self.has_previous_choice() {
            self.show_previous_choice();
        }
        if self.has_next_choice() {
            self.show_next_choice();
        }
    }

    /// Setting current buttons, and clearing.
    fn update(&mut self, ctx: &mut EventCtx) {
        self.set_buttons(ctx);
        self.clear(ctx);
    }

    /// Clearing the whole area and requesting repaint.
    fn clear(&mut self, ctx: &mut EventCtx) {
        self.pad.clear();
        ctx.request_paint();
    }

    fn last_page_index(&self) -> u8 {
        self.choices.len() as u8 - 1
    }

    fn has_previous_choice(&self) -> bool {
        self.page_counter > 0
    }

    fn has_next_choice(&self) -> bool {
        self.page_counter < self.last_page_index()
    }

    fn current_choice(&mut self) -> &mut T {
        &mut self.choices[self.page_counter as usize]
    }

    fn show_current_choice(&mut self) {
        self.choices[self.page_counter as usize].paint_center();
    }

    fn show_previous_choice(&mut self) {
        self.choices[(self.page_counter - 1) as usize].paint_left();
    }

    fn show_next_choice(&mut self) {
        self.choices[(self.page_counter + 1) as usize].paint_right();
    }

    fn decrease_page_counter(&mut self) {
        self.page_counter -= 1;
    }

    fn increase_page_counter(&mut self) {
        self.page_counter += 1;
    }

    /// Updating the visual state of the buttons after each event.
    /// All three buttons are handled based upon the current choice.
    /// If defined in the current choice, setting their text,
    /// whether they are long-pressed, and painting them.
    ///
    /// NOTE: ButtonController is handling the painting, and
    /// it will not repaint the buttons unless some of them changed.
    fn set_buttons(&mut self, ctx: &mut EventCtx) {
        // TODO: offer the possibility to change the buttons from the client
        // (button details could be changed in the same index)
        // Use-case: BIN button in PIN is deleting last digit if the PIN is not empty,
        // otherwise causing Cancel. Would be nice to allow deleting as a single click
        // and Cancel as HTC. PIN client would check if the PIN is empty/not and
        // adjust the HTC/not.

        let btn_layout = self.current_choice().btn_layout();
        self.buttons.mutate(ctx, |ctx, buttons| {
            buttons.set(ctx, btn_layout);
        });
    }
}

impl<T, const N: usize> Component for ChoicePage<T, N>
where
    T: ChoiceItem,
{
    type Msg = ChoicePageMsg;

    fn place(&mut self, bounds: Rect) -> Rect {
        let button_height = theme::FONT_BOLD.line_height() + 2;
        let (content_area, button_area) = bounds.split_bottom(button_height);
        self.pad.place(content_area);
        self.buttons.place(button_area);
        bounds
    }

    fn event(&mut self, ctx: &mut EventCtx, event: Event) -> Option<Self::Msg> {
        let button_event = self.buttons.event(ctx, event);

        if let Some(ButtonControllerMsg::Triggered(pos)) = button_event {
            match pos {
                ButtonPos::Left => {
                    if self.has_previous_choice() {
                        // Clicked BACK. Decrease the page counter.
                        self.decrease_page_counter();
                        self.update(ctx);
                    } else {
                        // Triggered LEFTmost button. Send event
                        self.clear(ctx);
                        return Some(ChoicePageMsg::LeftMost);
                    }
                }
                ButtonPos::Right => {
                    if self.has_next_choice() {
                        // Clicked NEXT. Increase the page counter.
                        self.increase_page_counter();
                        self.update(ctx);
                    } else {
                        // Triggered RIGHTmost button. Send event
                        self.clear(ctx);
                        return Some(ChoicePageMsg::RightMost);
                    }
                }
                ButtonPos::Middle => {
                    // Clicked SELECT. Send current choice index
                    self.clear(ctx);
                    return Some(ChoicePageMsg::Choice(self.page_counter));
                }
            }
        };
        None
    }

    fn paint(&mut self) {
        self.pad.paint();
        self.buttons.paint();
        self.paint_choices();
    }
}

#[cfg(feature = "ui_debug")]
impl<T, const N: usize> crate::trace::Trace for ChoicePage<T, N>
where
    T: ChoiceItem,
{
    fn trace(&self, t: &mut dyn crate::trace::Tracer) {
        t.open("ChoicePage");
        t.close();
    }
}

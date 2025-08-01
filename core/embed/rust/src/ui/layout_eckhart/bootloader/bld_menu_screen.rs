use crate::ui::{
    component::{Component, Event, EventCtx},
    geometry::{Alignment, Rect},
    layout_eckhart::component::Button,
    shape::Renderer,
};

use super::{
    super::{cshape::ScreenBorder, theme},
    bld_menu::BldMenuSelectionMsg,
    BldHeader, BldHeaderMsg, BldMenu,
};

const BUTTON_AREA_START: i16 = 56;
const BUTTON_SPACING: i16 = 8;

#[repr(u32)]
#[derive(Copy, Clone, ToPrimitive)]
pub enum BldMenuMsg {
    Close = 0xAABBCCDD,
    Reboot = 0x11223344,
    FactoryReset = 0x55667788,
    Bluetooth = 0x99AABBCC,
    PowerOff = 0x751A5BEF,
}

pub struct BldMenuScreen {
    header: BldHeader<'static>,
    menu: BldMenu,
    screen_border: Option<ScreenBorder>,
}

impl BldMenuScreen {
    pub fn new() -> Self {
        let bluetooth = Button::with_text("Pair new device".into())
            .styled(theme::bootloader::button_bld_menu())
            .with_text_align(Alignment::Start);
        let reboot = Button::with_text("Reboot".into())
            .styled(theme::bootloader::button_bld_menu())
            .with_text_align(Alignment::Start);
        let turnoff = Button::with_text("Power off".into())
            .styled(theme::bootloader::button_bld_menu())
            .with_text_align(Alignment::Start);
        let reset = Button::with_text("Factory reset".into())
            .styled(theme::bootloader::button_bld_menu_danger())
            .with_text_align(Alignment::Start);

        let menu = BldMenu::empty()
            .item(bluetooth)
            .item(reboot)
            .item(turnoff)
            .item(reset);
        Self {
            header: BldHeader::new("Bootloader".into()).with_close_button(),
            menu,
            screen_border: None,
        }
    }

    pub fn with_screen_border(mut self, screen_border: ScreenBorder) -> Self {
        self.screen_border = Some(screen_border);
        self
    }
}

impl Component for BldMenuScreen {
    type Msg = BldMenuMsg;

    fn place(&mut self, bounds: Rect) -> Rect {
        let (header_area, menu_area) = bounds.split_top(theme::HEADER_HEIGHT);

        self.header.place(header_area);
        self.menu.place(menu_area);
        bounds
    }

    fn event(&mut self, ctx: &mut EventCtx, event: Event) -> Option<Self::Msg> {
        if let Some(BldHeaderMsg::Cancelled) = self.header.event(ctx, event) {
            return Some(Self::Msg::Close);
        }

        if let Some(BldMenuSelectionMsg::Selected(n)) = self.menu.event(ctx, event) {
            match n {
                0 => return Some(Self::Msg::Bluetooth),
                1 => return Some(Self::Msg::Reboot),
                2 => return Some(Self::Msg::PowerOff),
                3 => return Some(Self::Msg::FactoryReset),
                _ => {}
            }
        }

        None
    }

    fn render<'s>(&'s self, target: &mut impl Renderer<'s>) {
        self.header.render(target);
        self.menu.render(target);
        if let Some(screen_border) = &self.screen_border {
            screen_border.render(u8::MAX, target);
        }
    }
}

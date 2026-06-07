use mod_api::*;

fn init(_ctx: &GameCtx) -> ModRegistration {
    let reg = ModRegistration::new("item_scroller_tfm2");
    reg
}

declare_mod!(init);

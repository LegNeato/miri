#![feature(lang_items, start)]
#![no_std]
// rustc-env: MIRI_NO_STD=1

#[start]
fn start(_: isize, _: *const *const u8) -> isize {
    for _ in 0..10 {}

    0
}

#[panic_handler]
fn panic_handler(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[lang = "eh_personality"]
fn eh_personality() {}

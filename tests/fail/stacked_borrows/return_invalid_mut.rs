// Make sure that we cannot return a `&mut` that got already invalidated.
fn foo(x: &mut (i32, i32)) -> &mut i32 {
    let xraw = x as *mut (i32, i32);
    let ret = unsafe { &mut (*xraw).1 };
    let _val = unsafe { *xraw }; // invalidate xref
    ret //~ ERROR: /reborrow .* tag does not exist in the borrow stack/
}

fn main() {
    foo(&mut (1, 2));
}

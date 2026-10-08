// A mod crate is linked only when the binary references it.
use hello_mod as _;

fn main() {
    pumpkin::run();
}

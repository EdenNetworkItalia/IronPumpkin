// The probe mod is linked only when the binary references its crate.
use parity_probe_rust as _;

fn main() {
    pumpkin::run();
}

use ferris_says::say; // from the previous step
use std::io::{stdout, BufWriter};

fn foo_ferris() {
    let stdout = stdout();
    let message = String::from("Hello fellow Rustaceans!");
    let width = message.chars().count()/3;

    let mut writer = BufWriter::new(stdout.lock());
    say(&message, width, &mut writer).unwrap();
}
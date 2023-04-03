use std::{env, io};

fn main() {
    assert_eq!("/foo a=b", &env::args().collect::<Vec<_>>().join(" "));
    assert_eq!(
        "what=up",
        &env::vars()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    assert_eq!(
        "hello, world!",
        &io::read_to_string(&mut io::stdin().lock()).unwrap()
    );

    print!("content-type: text/plain\n\nhola, mundo!\n");
}

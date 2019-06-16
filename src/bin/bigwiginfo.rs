use std::io;

use clap::{App, Arg};

use bigwig2::bigwig::BigWigRead;

fn main() -> io::Result<()> {
    let matches = App::new("BigWigInfo")
        .arg(Arg::with_name("bigwig")
                .help("the bigwig to get info for")
                .index(1)
                .required(true)
            )
        .get_matches();

    let bigwigpath = matches.value_of("bigwig").unwrap().to_owned();

    let bigwig = BigWigRead::from_file_and_attach(bigwigpath)?;
    println!("Header: {:?}", bigwig.info.header);

    Ok(())
}
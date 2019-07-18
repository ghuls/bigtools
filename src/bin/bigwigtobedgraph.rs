#![feature(async_await)]

use std::fs::File;
use std::io::{self, Write};

use clap::{App, Arg};

use futures::future::FutureExt;

use bigwig2::bigwig::BigWigRead;
use bigwig2::tempfilebuffer::TempFileBuffer;

pub fn write_bg(bigwig: BigWigRead, mut out_file: File) -> std::io::Result<()> {
    let chrom_files: Vec<io::Result<(_, TempFileBuffer)>> = bigwig.get_chroms().into_iter().map(|chrom| {
        let mut bigwig = bigwig.clone();
        let (buf, file) = TempFileBuffer::new()?;
        let mut writer = std::io::BufWriter::new(file);
        let file_future = async move || -> io::Result<()> {
            for raw_val in bigwig.get_interval(&chrom.name, 0, chrom.length)? {
                let val = raw_val?;
                writer.write_fmt(format_args!("{}\t{}\t{}\t{}\n", chrom.name, val.start, val.end, val.value))?;
            }
            Ok(())
        };
        let (remote, handle) = file_future().remote_handle();
        std::thread::spawn(move || {
            futures::executor::block_on(remote);
        });
        Ok((handle,buf))
    }).collect::<Vec<_>>();

    for res in chrom_files {
        let (f, mut buf) = res.unwrap();
        buf.switch(out_file).unwrap();
        futures::executor::block_on(f).unwrap();
        out_file = buf.await_file();
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let matches = App::new("BigWigToBedGraph")
        .arg(Arg::with_name("bigwig")
                .help("the bigwig to get convert to bedgraph")
                .index(1)
                .required(true)
            )
        .arg(Arg::with_name("bedgraph")
                .help("the path of the bedgraph to output to")
                .index(2)
                .required(true)
            )
        .get_matches();

    let bigwigpath = matches.value_of("bigwig").unwrap().to_owned();
    let bedgraphpath = matches.value_of("bedgraph").unwrap().to_owned();

    let bigwig = BigWigRead::from_file_and_attach(bigwigpath)?;
    let bedgraph = File::create(bedgraphpath)?;

    write_bg(bigwig, bedgraph)?;

    Ok(())
}

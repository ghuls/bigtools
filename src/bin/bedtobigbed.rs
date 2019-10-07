use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

use clap::{App, Arg};

use bigwig2::bigwig::{BigBedWrite, WriteGroupsError};
use bigwig2::bedparser::{self, BedParser};

fn main() -> Result<(), WriteGroupsError> {
    let matches = App::new("BedToBigBed")
        .arg(Arg::with_name("bed")
                .help("the n to convert to a bigbed")
                .index(1)
                .required(true)
            )
        .arg(Arg::with_name("chromsizes")
                .help("A chromosome sizes file. Each line should be have a chromosome and its size in bases, separated by whitespace.")
                .index(2)
                .required(true)
            )
        .arg(Arg::with_name("output")
                .help("The output bigbed path")
                .index(3)
                .required(true)
            )
        .get_matches();

    let bedpath = matches.value_of("bed").unwrap().to_owned();
    let chrom_map = matches.value_of("chromsizes").unwrap().to_owned();
    let bigwigpath = matches.value_of("output").unwrap().to_owned();

    let outb = BigBedWrite::create_file(bigwigpath);
    let chrom_map: HashMap<String, u32> = BufReader::new(File::open(chrom_map)?)
        .lines()
        .filter(|l| match l { Ok(s) => !s.is_empty(), _ => true })
        .map(|l| {
            let words = l.expect("Split error");
            let mut split = words.split_whitespace();
            (split.next().expect("Missing chrom").to_owned(), split.next().expect("Missing size").parse::<u32>().unwrap())
        })
        .collect();

    let infile = File::open(bedpath)?;
    let vals_iter = BedParser::from_file(infile);
    let chsi = bedparser::get_chromgroupstreamingiterator(vals_iter, outb.options.clone(), chrom_map.clone());
    outb.write_groups(chrom_map, chsi)?;

    Ok(())
}
use std::collections::{BTreeMap, HashMap};
use std::io;

use clap::{App, Arg};

use bigwig2::bigwig::BigWigWriteOptions;
use bigwig2::bigwig::ChromGroupReadStreamingIterator;
use bigwig2::chromvalues::ChromValues;
use bigwig2::bigwig::{BigWigRead, BigWigWrite};
use bigwig2::bigwig::Value;
use bigwig2::bigwig::ChromGroupRead;

use bigwig2::idmap::IdMap;

use bigwig2::utils::merge_sections_many;

pub fn get_merged_values(bigwigs: Vec<BigWigRead>, options: BigWigWriteOptions) -> io::Result<(impl ChromGroupReadStreamingIterator + std::marker::Send, HashMap<String, u32>)> {
    // Get sizes for each and check that all files (that have the chrom) agree
    // Check that all chrom sizes match for all files
    let mut chrom_sizes = BTreeMap::new();
    let mut chrom_map = HashMap::new();
    for chrom in bigwigs.iter().flat_map(BigWigRead::get_chroms).map(|c| c.name) {
        if chrom_sizes.get(&chrom).is_some() {
            continue;
        }
        let (sizes, bws): (Vec<_>, Vec<_>) = bigwigs.iter().map(|w| {
            let chroms = w.get_chroms();
            let res = chroms.iter().find(|v| v.name == chrom);
            match res {
                Some(s) => Some((s.length, w.clone())),
                None => None,
            }
        }).filter_map(|x| x).unzip();
        let size = sizes[0];
        if !sizes.iter().all(|s| *s == size) {
            eprintln!("Chrom '{:?}' had different sizes in the bigwig files. (Are you using the same assembly?)", chrom);
            return Err(io::Error::new(io::ErrorKind::Other, "Invalid input (nonmatching chroms)"));
        }

        chrom_sizes.insert(chrom.clone(), (size, bws));
        chrom_map.insert(chrom.clone(), size);
    }

    let mut chrom_ids = IdMap::new();
    let chrom_ids = chrom_sizes.iter().map(|(c, _)| chrom_ids.get_id(c.clone())).collect::<Vec<_>>().into_iter();

    struct MergingValues<I: Iterator<Item=Value> + Send> {
        iter: std::iter::Peekable<I>,
    }

    impl<I: Iterator<Item=Value> + Send> ChromValues for MergingValues<I> {
        fn next(&mut self) -> io::Result<Option<Value>> {
            Ok(self.iter.next())
        }

        fn peek(&mut self) -> Option<&Value> {
            self.iter.peek()
        }
    }

    struct ChromGroupReadStreamingIteratorImpl {
        pool: futures::executor::ThreadPool,
        options: BigWigWriteOptions,
        iter: Box<Iterator<Item=((String, (u32, Vec<BigWigRead>)), u32)> + Send>,
    }

    impl ChromGroupReadStreamingIterator for ChromGroupReadStreamingIteratorImpl {
        fn next(&mut self) -> io::Result<Option<ChromGroupRead>> {
            let next = self.iter.next();
            match next {
                Some(((chrom, (size, bws)), chrom_id)) => {
                    let current_chrom = chrom.clone();
                    let iters: Vec<_> = bws.into_iter().map(move |b| b.get_interval_move(&chrom, 1, size)).collect::<io::Result<Vec<_>>>()?;
                    let mergingvalues = MergingValues { iter: merge_sections_many(iters).filter(|x| x.value != 0.0).peekable() };
                    Ok(Some(BigWigWrite::read_group(current_chrom, chrom_id, mergingvalues, self.pool.clone(), self.options.clone()).unwrap()))
                },
                None => {
                    return Ok(None)       
                },
            }
        }
    }

    let group_iter = ChromGroupReadStreamingIteratorImpl {
        pool: futures::executor::ThreadPoolBuilder::new().pool_size(4).create().expect("Unable to create thread pool."),
        options: options,
        iter: Box::new(chrom_sizes.into_iter().zip(chrom_ids)),
    };

    Ok((group_iter, chrom_map))
}

fn main() -> io::Result<()> {
    let matches = App::new("BigWigMerge")
        .arg(Arg::with_name("output")
                .help("the path of the merged output bigwig")
                .index(1)
                .required(true)
            )
        .arg(Arg::with_name("bigwig")
                .short("b")
                .help("the path of an input bigwig to merge")
                .multiple(true)
                .takes_value(true)
                .required(true)
            )
        .get_matches();

    let output = matches.value_of("output").unwrap().to_owned();
    let bigwigs = matches
        .values_of("bigwig")
        .unwrap()
        .map(|b| BigWigRead::from_file_and_attach(b.to_owned()))
        .collect::<Result<Vec<_>, _>>()?;

    let outb = BigWigWrite::create_file(output)?;
    let (all_values, chrom_map) = get_merged_values(bigwigs, outb.options.clone())?;
    outb.write_groups(chrom_map, all_values)?;

    //TODO: fails with too many open files
    Ok(())
}
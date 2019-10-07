use std::io::{self};

#[test]
fn test() -> io::Result<()> {
    use std::collections::HashMap;
    use std::fs::File;
    use std::path::PathBuf;

    use tempfile;

    use bigwig2::bedgraphparser::{self, BedGraphParser};
    use bigwig2::chromvalues::{ChromGroups, ChromValues};
    use bigwig2::bigwig::{BBIRead, BigWigRead, BigWigWrite};

    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("resources/test");

    let mut single_chrom_bedgraph = dir.clone();
    single_chrom_bedgraph.push("single_chrom.bedGraph");
    
    let first = {
        let infile = File::open(single_chrom_bedgraph.clone())?;
        let mut vals_iter = BedGraphParser::from_file(infile);
        let (_, mut group) = vals_iter.next()?.unwrap();
        group.next()?.unwrap()
    };

    let infile = File::open(single_chrom_bedgraph)?;
    let tempfile = tempfile::NamedTempFile::new()?;
    let vals_iter = BedGraphParser::from_file(infile);
    let outb = BigWigWrite::create_file(tempfile.path().to_string_lossy().to_string());

    let mut chrom_map = HashMap::new();
    chrom_map.insert("chr17".to_string(), 83257441);
    let chsi = bedgraphparser::get_chromgroupstreamingiterator(vals_iter, outb.options.clone(), chrom_map.clone());
    outb.write_groups(chrom_map, chsi).unwrap();

    let mut bwread = BigWigRead::from_file_and_attach(tempfile.path().to_string_lossy().to_string()).unwrap(); 

    let chroms = bwread.get_chroms();
    assert_eq!(chroms.len(), 1);
    assert_eq!(chroms[0].name, "chr17");
    assert_eq!(chroms[0].length, 83257441);

    let mut intervals = bwread.get_interval("chr17", 0, 83257441)?;
    let first_interval = intervals.next().unwrap().unwrap();
    assert_eq!(first.start, first_interval.start);
    assert_eq!(first.end, first_interval.end);
    assert_eq!(first.value, first_interval.value);

    Ok(())
}

#[test]
fn test_multi() -> io::Result<()> {
    use std::collections::HashMap;
    use std::fs::File;
    use std::path::PathBuf;

    use tempfile;

    use bigwig2::bedgraphparser::{self, BedGraphParser};
    use bigwig2::bigwig::{BBIRead, BigWigRead, BigWigWrite};

    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("resources/test");

    let mut multi_chrom_bedgraph = dir.clone();
    multi_chrom_bedgraph.push("multi_chrom.bedGraph");

    let infile = File::open(multi_chrom_bedgraph)?;
    let tempfile = tempfile::NamedTempFile::new()?;
    let vals_iter = BedGraphParser::from_file(infile);
    let outb = BigWigWrite::create_file(tempfile.path().to_string_lossy().to_string());

    let mut chrom_map = HashMap::new();
    chrom_map.insert("chr1".to_string(), 248956422);
    chrom_map.insert("chr2".to_string(), 242193529);
    chrom_map.insert("chr3".to_string(), 198295559);
    chrom_map.insert("chr4".to_string(), 190214555);
    chrom_map.insert("chr5".to_string(), 181538259);
    chrom_map.insert("chr6".to_string(), 170805979);
    let chsi = bedgraphparser::get_chromgroupstreamingiterator(vals_iter, outb.options.clone(), chrom_map.clone());
    outb.write_groups(chrom_map, chsi).unwrap();

    let mut bwread = BigWigRead::from_file_and_attach(tempfile.path().to_string_lossy().to_string()).unwrap(); 

    let chroms = bwread.get_chroms();
    assert_eq!(chroms.len(), 6);

    assert_eq!(bwread.get_interval("chr1", 0, 248956422).unwrap().count(), 200);
    assert_eq!(bwread.get_interval("chr6", 0, 170805979).unwrap().count(), 2000);

    Ok(())
}
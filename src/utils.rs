use std::io;

use crate::bigwig::Value;

/// Returns:
///  (val, None, None, overhang or None) when merging two does not break up one, and may or may not add an overhang (one.start == two.start)
///  (val, val, val or None, overhang or None) when merging two breaks up one, and may or may not add an overhang (one.start < two.start or one.end > two.end)
/// The overhang may equal the previous value
///
/// # Panics
/// Panics if the two Values do not overlap.
pub fn merge_into(one: Value, two: Value) -> (Value, Option<Value>, Option<Value>, Option<Value>) {
    if one.end <= two.start {
        panic!("No overlap.");
    }
    if one.start == two.start {
        // |--
        // |--
        if one.end == two.end {
            // |---|
            // |---|
            (
                Value {
                    start: one.start,
                    end: one.end,
                    value: one.value + two.value,
                },
                None,
                None,
                None,
            )
        } else if one.end < two.end {
            // |--|
            // |---|
            (
                Value {
                    start: one.start,
                    end: one.end,
                    value: one.value + two.value,
                },
                None,
                None,
                Some(Value {
                    start: one.end,
                    end: two.end,
                    value: two.value,
                }),
            )
        } else {
            // |---|
            // |--|
            if two.value == 0.0 {
                (one, None, None, None)
            } else {
                (
                    Value {
                        start: two.start,
                        end: two.end,
                        value: one.value + two.value,
                    },
                    Some(Value {
                        start: two.end,
                        end: one.end,
                        value: one.value,
                    }),
                    None,
                    None,
                )
            }
        }
    } else if one.start < two.start {
        // |--
        //  |--
        if one.end == two.end {
            // |---|
            //  |--|
            if two.value == 0.0 {
                (
                    Value {
                        start: one.start,
                        end: one.end,
                        value: one.value,
                    },
                    None,
                    None,
                    None,
                )
            } else {
                (
                    Value {
                        start: one.start,
                        end: two.start,
                        value: one.value,
                    },
                    Some(Value {
                        start: two.start,
                        end: two.end,
                        value: one.value + two.value,
                    }),
                    None,
                    None,
                )
            }
        } else if one.end < two.end {
            // |---|
            //  |---|
            if one.value == 0.0 && two.value == 0.0 {
                let end = one.end;
                (
                    one,
                    None,
                    None,
                    Some(Value {
                        start: end,
                        end: two.end,
                        value: 0.0,
                    }),
                )
            } else if one.value == 0.0 {
                (
                    Value {
                        start: one.start,
                        end: two.start,
                        value: 0.0,
                    },
                    Some(Value {
                        start: two.start,
                        end: one.end,
                        value: two.value,
                    }),
                    None,
                    Some(Value {
                        start: one.end,
                        end: two.end,
                        value: two.value,
                    }),
                )
            } else if two.value == 0.0 {
                let end = one.end;
                (
                    one,
                    None,
                    None,
                    Some(Value {
                        start: end,
                        end: two.end,
                        value: 0.0,
                    }),
                )
            } else {
                (
                    Value {
                        start: one.start,
                        end: two.start,
                        value: one.value,
                    },
                    Some(Value {
                        start: two.start,
                        end: one.end,
                        value: one.value + two.value,
                    }),
                    None,
                    Some(Value {
                        start: one.end,
                        end: two.end,
                        value: two.value,
                    }),
                )
            }
        } else {
            // |----|
            //  |--|
            if two.value == 0.0 {
                (one, None, None, None)
            } else {
                (
                    Value {
                        start: one.start,
                        end: two.start,
                        value: one.value,
                    },
                    Some(Value {
                        start: two.start,
                        end: two.end,
                        value: one.value + two.value,
                    }),
                    Some(Value {
                        start: two.end,
                        end: one.end,
                        value: one.value,
                    }),
                    None,
                )
            }
        }
    } else {
        //  |--
        // |--
        if one.end == two.end {
            //  |--|
            // |---|
            if one.value == 0.0 {
                (two, None, None, None)
            } else {
                (
                    Value {
                        start: two.start,
                        end: one.start,
                        value: two.value,
                    },
                    Some(Value {
                        start: one.start,
                        end: one.end,
                        value: one.value + two.value,
                    }),
                    None,
                    None,
                )
            }
        } else if one.end < two.end {
            //  |--|
            // |----|
            if one.value == 0.0 {
                (two, None, None, None)
            } else {
                (
                    Value {
                        start: two.start,
                        end: one.start,
                        value: two.value,
                    },
                    Some(Value {
                        start: one.start,
                        end: one.end,
                        value: one.value + two.value,
                    }),
                    None,
                    Some(Value {
                        start: one.end,
                        end: two.end,
                        value: two.value,
                    }),
                )
            }
        } else {
            //  |---|
            // |---|
            if one.value == 0.0 && two.value == 0.0 {
                (
                    Value {
                        start: two.start,
                        end: one.end,
                        value: 0.0,
                    },
                    None,
                    None,
                    None,
                )
            } else if one.value == 0.0 {
                let start = two.end;
                (
                    two,
                    Some(Value {
                        start,
                        end: one.end,
                        value: one.value,
                    }),
                    None,
                    None,
                )
            } else if two.value == 0.0 {
                (
                    Value {
                        start: two.start,
                        end: one.start,
                        value: 0.0,
                    },
                    Some(Value {
                        start: one.start,
                        end: one.end,
                        value: one.value,
                    }),
                    None,
                    None,
                )
            } else {
                (
                    Value {
                        start: two.start,
                        end: one.start,
                        value: two.value,
                    },
                    Some(Value {
                        start: one.start,
                        end: two.end,
                        value: one.value + two.value,
                    }),
                    Some(Value {
                        start: two.end,
                        end: one.end,
                        value: one.value,
                    }),
                    None,
                )
            }
        }
    }
}

struct ValueIter<I>
where
    I: Iterator<Item = io::Result<Value>> + Send,
{
    error: io::Result<()>,
    sections: Vec<(I, Option<Value>)>,
    next_sections: Option<Box<dyn Iterator<Item = Value> + Send>>,
    last_val: Option<Value>,
    next_start: u32,
}

impl<I> Iterator for ValueIter<I>
where
    I: Iterator<Item = io::Result<Value>> + Send,
{
    type Item = Value;

    fn next(&mut self) -> Option<Value> {
        if let Some(buf) = &mut self.next_sections {
            let next = buf.next();
            match next {
                None => self.next_sections = None,
                Some(_) => return next,
            }
        }

        const DATA_SIZE: usize = 50000;
        loop {
            let current_start = self.next_start;
            self.next_start = current_start + DATA_SIZE as u32;

            let mut data = vec![0f32; DATA_SIZE];
            let mut max_sections: usize = 0;
            let mut all_none = true;
            'sections: for (section, last) in &mut self.sections {
                'section: loop {
                    let next_val = match last.take() {
                        Some(next_val) => next_val,
                        None => match section.next() {
                            Some(Ok(x)) => x,
                            Some(Err(e)) => {
                                self.error = Err(e);
                                continue 'section;
                            }
                            None => continue 'sections,
                        },
                    };
                    all_none = false;

                    let data_start = (current_start.max(next_val.start) - current_start) as usize;
                    if data_start >= DATA_SIZE {
                        *last = Some(next_val);
                        break 'section;
                    }
                    let data_end = DATA_SIZE.min((next_val.end - current_start) as usize);
                    let value = next_val.value;
                    for i in &mut data[data_start..data_end] {
                        *i += value
                    }
                    max_sections += 1;
                    if (next_val.end - current_start) as usize >= DATA_SIZE {
                        *last = Some(next_val);
                        break 'section;
                    }
                }
            }

            // TODO: coverage so can take average, or 'real' zeros
            let mut next_sections: Vec<Value> = Vec::with_capacity(max_sections * 2);
            let mut current: Option<(u32, u32, f32)> = None;
            for (idx, i) in data[..].iter().enumerate() {
                match &mut current {
                    None => {
                        current = Some((
                            idx as u32 + current_start,
                            idx as u32 + current_start + 1,
                            *i,
                        ))
                    }
                    Some(c) => {
                        if (c.2 - *i).abs() < std::f32::EPSILON {
                            c.1 += 1;
                        } else {
                            if c.2 != 0.0 {
                                next_sections.push(Value {
                                    start: c.0,
                                    end: c.1,
                                    value: c.2,
                                });
                            }
                            current = Some((
                                idx as u32 + current_start,
                                idx as u32 + current_start + 1,
                                *i,
                            ));
                        }
                    }
                }
            }
            if let Some(c) = &mut current {
                if c.2 != 0.0 {
                    next_sections.push(Value {
                        start: c.0,
                        end: c.1,
                        value: c.2,
                    });
                }
            }

            let insert_into_queue = |queue: &mut Vec<Value>, next_val: Value| {
                let mut insert_val = next_val;
                'insert: loop {
                    if queue.is_empty() || queue.last().unwrap().end <= insert_val.start {
                        queue.push(insert_val);
                        return;
                    }

                    for (idx, queued) in queue.iter_mut().enumerate() {
                        // We know that next_val is somewhere before where the last queued val ends
                        // It's either:
                        // - before all queued items (checked in the first loop iteration)
                        // - between two items
                        // - overlapping one or more items

                        // Check if next_val is strictly before the current val
                        // If this is not the first item, we have already checked that it does not overlap others
                        if insert_val.end <= queued.start {
                            queue.insert(idx, insert_val);
                            return;
                        }
                        // If the end of the queued val is strictly before next_val, no need to do anything. (If it's before the next item, we will catch that next loop iteration)
                        if queued.end <= insert_val.start {
                            continue;
                        }
                        // We now know that next_val overlaps with the current item
                        let nvq = std::mem::replace(
                            queued,
                            Value {
                                start: 0,
                                end: 0,
                                value: 0.0,
                            },
                        );
                        // See merge_into for what these are
                        // In short: one, two, and three are strictly contained within the current val's start-end, while overhang is anything left over
                        let (one, two, three, overhang) = merge_into(nvq, insert_val);
                        std::mem::replace(queued, one);

                        // If these exist, they don't change any of the queue after the current item
                        if let Some(th) = three {
                            queue.insert(idx + 1, th);
                        }
                        if let Some(tw) = two {
                            queue.insert(idx + 1, tw);
                        }

                        // If we have an overhang, we have to propagate this down the queue
                        match overhang {
                            Some(o) => {
                                insert_val = o;
                                continue 'insert;
                            }
                            None => return,
                        }
                    }
                    unreachable!();
                }
            };

            let last_val = self.last_val.take();
            if let Some(last) = last_val {
                insert_into_queue(&mut next_sections, last);
            }

            if !next_sections.is_empty() {
                self.last_val = Some(next_sections.remove(next_sections.len() - 1));
            }

            if !next_sections.is_empty() {
                // TODO: will split values across boundary line
                self.next_sections = Some(Box::new(next_sections.into_iter()));
                return self.next_sections.as_mut().unwrap().next();
            }
            if all_none {
                return self.last_val.take();
            }
        }
    }
}

pub fn merge_sections_many<I>(sections: Vec<I>) -> impl Iterator<Item = Value> + Send
where
    I: Iterator<Item = io::Result<Value>> + Send,
{
    ValueIter {
        // TODO: this isn't used right now
        error: Ok(()),
        sections: sections.into_iter().map(|s| (s, None)).collect(),
        next_sections: None,
        last_val: None,
        next_start: 0,
    }
}

struct FillValues<I>
where
    I: Iterator<Item = io::Result<Value>>,
{
    iter: I,
    last_val: Option<Value>,
    expected_end: Option<u32>,
    last_end: u32,
}

impl<I> Iterator for FillValues<I>
where
    I: Iterator<Item = io::Result<Value>> + Send,
{
    type Item = io::Result<Value>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(last) = self.last_val.take() {
            self.last_end = last.end;
            return Some(Ok(last));
        }
        let next = self.iter.next();
        match next {
            Some(Ok(next)) => {
                if next.start > self.last_end {
                    let last = self.last_end;
                    self.last_end = next.start;
                    self.last_val.replace(next);
                    Some(Ok(Value {
                        start: last,
                        end: self.last_end,
                        value: 0.0,
                    }))
                } else {
                    self.last_end = next.end;
                    Some(Ok(next))
                }
            }
            Some(_) => next,
            None => match self.expected_end {
                None => None,
                Some(expected_end) => {
                    if self.last_end < expected_end {
                        let last = self.last_end;
                        self.last_end = expected_end;
                        Some(Ok(Value {
                            start: last,
                            end: expected_end,
                            value: 0.0,
                        }))
                    } else {
                        None
                    }
                }
            },
        }
    }
}

/// Fills any space between `Value`s with `0.0`s.
/// Note: Output values will not be merged if any input Values are `0.0`
pub fn fill<I>(iter: I) -> impl Iterator<Item = io::Result<Value>> + Send
where
    I: Iterator<Item = io::Result<Value>> + Send,
{
    FillValues {
        iter,
        last_val: None,
        expected_end: None,
        last_end: 0,
    }
}

/// Fills any space between `Value`s with `0.0`s. This will also pad the start and end with `0.0`s if they do not exist.
/// Note: Output values will not be merged if any input Values are `0.0`
///
/// If the start > the end of the first value, it will be ignored.
pub fn fill_start_to_end<I>(
    iter: I,
    start: u32,
    end: u32,
) -> impl Iterator<Item = io::Result<Value>> + Send
where
    I: Iterator<Item = io::Result<Value>> + Send,
{
    FillValues {
        iter,
        last_val: None,
        expected_end: Some(end),
        last_end: start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate test;

    #[test]
    fn test_merge_many() {
        let end = 15000;
        let first = generate_sections_seq(50, end, 1234);
        let second = generate_sections_seq(50, end, 12345);
        //println!("Running merge many with: \n{:?} \n{:?}", first, second);
        let merged = merge_sections_many(vec![
            first.into_iter().map(Result::Ok),
            second.into_iter().map(Result::Ok),
        ])
        .collect::<Vec<_>>();
        //println!("\nMerged (many): {:?}\n", merged);
        let mut last_end = 0;
        let mut last_val = None;
        for val in merged {
            assert!(last_end <= val.start);
            if let Some(last_val) = last_val {
                assert!(last_val != val.value);
            }
            last_end = val.end;
            last_val = Some(val.value);
        }
        assert!(last_end == end);
    }

    #[bench]
    fn bench_merge_many(b: &mut test::Bencher) {
        let first = generate_sections_seq(50, 150000, 1234);
        let second = generate_sections_seq(50, 150000, 12345);
        b.iter(|| {
            let merged = merge_sections_many(vec![
                first.clone().into_iter().map(Result::Ok),
                second.clone().into_iter().map(Result::Ok),
            ]);
            let mut last_start = 0;
            for val in merged {
                assert!(last_start <= val.start);
                last_start = val.start;
            }
        });
    }

    #[bench]
    fn bench_merge_many_skiplarge(b: &mut test::Bencher) {
        let first = generate_sections_seq(50, 15000, 1234);
        let second = generate_sections_seq_skip(50, 15000, 12345, 50.0, 200.0);
        let third = generate_sections_seq_skip(50, 15000, 123456, 0.0, 0.0);
        b.iter(|| {
            let merged = merge_sections_many(vec![
                first.clone().into_iter().map(Result::Ok),
                second.clone().into_iter().map(Result::Ok),
                third.clone().into_iter().map(Result::Ok),
            ]);
            let mut last_start = 0;
            for val in merged {
                assert!(last_start <= val.start);
                last_start = val.start;
            }
        });
    }

    #[test]
    fn can_gen() {
        let _sections = generate_sections_seq(50, 150, 1234);
        assert!(_sections.last().map(|v| v.end).unwrap_or(0) == 150);
    }

    fn generate_sections_seq_skip(
        start: u32,
        end: u32,
        seed: u64,
        skip: f32,
        size: f32,
    ) -> Vec<Value> {
        use rand::prelude::*;

        let mut out = vec![];

        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);

        let mut curr = start;
        loop {
            let value: f32 = rng.gen();
            let size = (rng.gen::<f32>() * size).floor() as u32 + 5;
            let skip = 0.max((rng.gen::<f32>() * skip).floor() as i32 + -7) as u32;

            let curr_end = end.min(curr + size);
            out.push(Value {
                start: curr,
                end: curr_end,
                value,
            });
            if end <= curr_end + skip {
                break;
            } else {
                curr = curr + size + skip;
            }
        }
        out
    }

    fn generate_sections_seq(start: u32, end: u32, seed: u64) -> Vec<Value> {
        use rand::prelude::*;

        let mut out = vec![];

        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);

        let mut curr = start;
        loop {
            let value: f32 = rng.gen();
            let size = (rng.gen::<f32>() * 20.0).floor() as u32 + 5;
            let skip = 0.max((rng.gen::<f32>() * 10.0).floor() as i32 + -7) as u32;

            let curr_end = end.min(curr + size);
            out.push(Value {
                start: curr,
                end: curr_end,
                value,
            });
            // Ensure that we also have a value at the last base
            if end < curr_end {
                out.push(Value {
                    start: curr_end - 1,
                    end: curr_end,
                    value: rng.gen(),
                });
                break;
            } else if end <= curr_end + skip {
                break;
            } else {
                curr = curr + size + skip;
            }
        }
        out
    }

    #[test]
    fn test_fill() {
        let intervals: Vec<io::Result<Value>> = vec![
            Ok(Value {
                start: 10,
                end: 15,
                value: 0.5,
            }),
            Ok(Value {
                start: 20,
                end: 30,
                value: 0.7,
            }),
            Ok(Value {
                start: 30,
                end: 35,
                value: 0.9,
            }),
            Err(io::Error::new(io::ErrorKind::Other, "Test error")),
        ];

        let mut iter = fill(intervals.into_iter());
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 0,
                end: 10,
                value: 0.0
            }
        );
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 10,
                end: 15,
                value: 0.5
            }
        );
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 15,
                end: 20,
                value: 0.0
            }
        );
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 20,
                end: 30,
                value: 0.7
            }
        );
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 30,
                end: 35,
                value: 0.9
            }
        );
        assert!(iter.next().unwrap().is_err());
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_fill_start_to_end() {
        let intervals: Vec<io::Result<Value>> = vec![
            Ok(Value {
                start: 10,
                end: 15,
                value: 0.5,
            }),
            Ok(Value {
                start: 20,
                end: 30,
                value: 0.7,
            }),
            Ok(Value {
                start: 30,
                end: 35,
                value: 0.9,
            }),
            Err(io::Error::new(io::ErrorKind::Other, "Test error")),
        ];

        let mut iter = fill_start_to_end(intervals.into_iter(), 5, 50);
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 5,
                end: 10,
                value: 0.0
            }
        );
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 10,
                end: 15,
                value: 0.5
            }
        );
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 15,
                end: 20,
                value: 0.0
            }
        );
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 20,
                end: 30,
                value: 0.7
            }
        );
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 30,
                end: 35,
                value: 0.9
            }
        );
        assert!(iter.next().unwrap().is_err());
        assert_eq!(
            iter.next().unwrap().unwrap(),
            Value {
                start: 35,
                end: 50,
                value: 0.0
            }
        );
        assert!(iter.next().is_none());
    }
}

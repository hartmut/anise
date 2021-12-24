/*
 * ANISE Toolkit
 * Copyright (C) 2021 Christopher Rabotin <christopher.rabotin@gmail.com> et al. (cf. AUTHORS.md)
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use crate::{parse_bytes_as, prelude::AniseError};
use std::convert::TryInto;

const RCRD_LEN: usize = 1024;
const DBL_SIZE: usize = 8;
const INT_SIZE: usize = 4;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Debug)]
pub struct DAF<'a> {
    pub idword: &'a str,
    pub internal_filename: &'a str,
    /// The number of integer components in each array summary.
    pub ni: usize,
    /// The number of double precision components in each array summary.
    pub nd: usize,
    /// The record number of the initial summary record in the file.
    pub fwrd: usize,
    /// The record number of the final summary record in the file.
    pub bwrd: usize,
    pub freeaddr: usize,
    pub endianness: Endianness,
    pub bytes: &'a [u8],
}

impl<'a> DAF<'a> {
    /// From https://naif.jpl.nasa.gov/pub/naif/toolkit_docs/C/req/daf.html#Structure
    pub fn parse(bytes: &'a [u8]) -> Result<Self, AniseError> {
        let locidw = std::str::from_utf8(&bytes[0..8]).or_else(|_| {
            Err(AniseError::InvalidDAF(
                "Could not parse header (first 8 bytes)".to_owned(),
            ))
        })?;

        let daftype: Vec<&str> = locidw.split("/").collect();
        if daftype.len() != 2 {
            return Err(AniseError::InvalidDAF(format!(
                "Malformed header string: `{}`",
                locidw
            )));
        } else if daftype[1].trim() != "SPK" {
            return Err(AniseError::InvalidDAF(format!(
                "Cannot parse a NAIF DAF of type: `{}`",
                locidw
            )));
        } else if daftype[1].trim() != "PCK" {
            println!("Good luck on the PCK debugging");
        }

        // We need to figure out if this file is big or little endian before we can convert some byte arrays into integer
        let str_endianness = std::str::from_utf8(&bytes[88..96]).or_else(|_| {
            Err(AniseError::InvalidDAF(
                "Could not parse endianness".to_owned(),
            ))
        })?;

        let endianness = if str_endianness == "LTL-IEEE" {
            Endianness::Little
        } else if str_endianness == "BIG-IEEE" {
            Endianness::Big
        } else {
            return Err(AniseError::InvalidDAF(format!(
                "Could not understand endianness: `{}`",
                str_endianness
            )));
        };

        // Note that we parse as u32 to make sure that it's a 32-bit integer. The docs don't specify if it's signed or not,
        // but it works in either case (I guess that the sign bit is still present but set to zero?)
        let nd = parse_bytes_as!(u32, &bytes[8..8 + INT_SIZE], endianness) as usize;
        let ni = parse_bytes_as!(u32, &bytes[12..12 + INT_SIZE], endianness) as usize;
        let fwrd = parse_bytes_as!(u32, &bytes[76..76 + INT_SIZE], endianness) as usize;
        let bwrd = parse_bytes_as!(u32, &bytes[80..80 + INT_SIZE], endianness) as usize;
        let freeaddr = parse_bytes_as!(u32, &bytes[84..84 + INT_SIZE], endianness) as usize;

        let locifn = std::str::from_utf8(&bytes[16..76])
            .or_else(|_| Err(AniseError::InvalidDAF("Could not parse locifn".to_owned())))?;

        // Ignore the FTPSTR (seems null in the DE440 and the padding to complete the record).

        Ok(Self {
            idword: locidw.trim(),
            internal_filename: locifn.trim(),
            nd,
            ni,
            fwrd,
            bwrd,
            freeaddr,
            endianness,
            bytes,
        })
    }

    pub fn comments(&self) -> String {
        let mut rslt = String::new();
        // FWRD has the initial record of the summary. So we assume that all records between the second record and that one are comments
        for rid in 1..self.fwrd {
            match std::str::from_utf8(&self.bytes[rid * RCRD_LEN..(rid + 1) * RCRD_LEN]) {
                Ok(s) => rslt += &s.replace("\u{0}\u{0}", " ").replace("\u{0}", "\n").trim(),
                Err(e) => {
                    let valid_s = std::str::from_utf8(
                        &self.bytes[rid * RCRD_LEN..(rid * RCRD_LEN + e.valid_up_to())],
                    )
                    .unwrap();
                    rslt += valid_s
                        .replace("\u{0}\u{0}", " ")
                        .replace("\u{0}", "\n")
                        .trim()
                }
            }
        }

        rslt
    }

    /// The summaries are needed to decode the rest of the file
    pub fn summaries(&self) {
        // Each summary need to be read in bytes of 8*nd then 4*self.ni
        let single_summary_size = self.nd + (self.ni + 1) / 2;
        let num_summaries = 125 / single_summary_size;
        dbg!(single_summary_size);
        let mut record_num = self.fwrd;
        loop {
            if record_num == 0 {
                break;
            }
            let record = self.record(record_num);
            // Note that the segment control data are stored as f64 but need to be converted to usize
            let next_record = parse_bytes_as!(f64, &record[0..DBL_SIZE], self.endianness) as usize;
            let prev_record =
                parse_bytes_as!(f64, &record[8..8 + DBL_SIZE], self.endianness) as usize;
            let nsummaries =
                parse_bytes_as!(f64, &record[16..16 + DBL_SIZE], self.endianness) as usize;
            println!("{:?}", &record);
            dbg!(next_record, prev_record, nsummaries);
            dbg!(record.len());
            // Parse the data of the summary.
            let name_record = self.record(record_num + 1);
            let step = num_summaries;
            let length = DBL_SIZE * self.nd + INT_SIZE * self.ni;
            for i in (0..nsummaries * length).step_by(length) {
                let j = 3 * DBL_SIZE + i;
                let name = std::str::from_utf8(&name_record[i..i + length]).unwrap();
                if name.chars().nth(0).unwrap() == ' ' {
                    println!("WARNING: Parsing might be wrong because the first character of the name summary is a space: `{}`", name);
                    println!(
                        "Full name data: `{}`",
                        std::str::from_utf8(&name_record[..1000]).unwrap()
                    );
                }
                let summary_data = &record[j..j + length];
                println!("`{}` => {:?}", name.trim(), summary_data);
                let mut nd = 0;
                for double_data in summary_data[0..DBL_SIZE * self.nd].chunks(8) {
                    dbg!(parse_bytes_as!(f64, &double_data, self.endianness));
                    nd += 1;
                }
                assert_eq!(nd, self.nd);
                let mut ni = 0;
                for int_data in
                    summary_data[DBL_SIZE * self.nd..(self.nd * 8 + self.ni * 4)].chunks(4)
                {
                    dbg!(parse_bytes_as!(i32, &int_data, self.endianness));
                    ni += 1;
                }
                assert_eq!(ni, self.ni);
            }
            println!("{:?}", &name_record);
            record_num = next_record;
        }

        dbg!(num_summaries);
    }

    /// Records are indexed from one!!
    fn record(&self, num: usize) -> &'a [u8] {
        let start_idx = num * RCRD_LEN - RCRD_LEN;
        &self.bytes[start_idx..start_idx + RCRD_LEN]
    }
}

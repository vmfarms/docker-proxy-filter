use crate::config::ContainerLabels;
use core::num;
use std::collections::HashMap;
use tracing::*;

pub fn strings_in_strings(ref_strings: &Vec<String>, contains_strings: &Vec<String>) -> bool {
    let matched_str = ref_strings.iter().find(|x| contains_strings.iter().any(|y| { 
        let contains = x.contains(y);
        if contains {
            debug!("'{}' found in {}", y, x);
        }
        contains
    }));

    matched_str.is_some()
}

pub fn label_match(ref_map: &HashMap<String,String>, contains_map: &ContainerLabels) -> bool {
    if contains_map.is_empty() {
        return true
    }
    if ref_map.is_empty() {
        return false
    }

    for(k, v) in ref_map.iter() {
        if contains_map.iter().any(|(ck, cv)| {
            if k.contains(ck) {
                let val_match = match cv {
                    Some(val) => {
                        if v.contains(val) {
                            debug!("Label {}={} contains label key filter '{}' and label value filter '{}'", k,v, ck, val);
                            return true;
                        }
                        false
                    }
                    None => {
                        debug!("Label {}={} contains label key filter '{}'", k,v, ck);
                        true
                    }
                };
                return val_match
            }
            return false
        }) {
            return true
        }
    }

    return false
}

pub fn short_id(s: &String) -> String {
    return format!("{start}...{end}", start = &s[..6], end = &s[s.len() - 6..]);
}
pub fn parse_comma_separated_list(input: &str, to_lowercase: bool) -> Vec<String> {
    if input.is_empty() {
        Vec::new()
    } else {
        if to_lowercase {
            input.split(',').map(|s| s.trim().to_lowercase().to_string()).collect()
        } else {
            input.split(',').map(|s| s.trim().to_string()).collect()
        }
    }
}

pub fn parse_key_value_pairs(input: &str) -> Vec<(String, String)> {
    if input.is_empty() {
        Vec::new()
    } else {
        input
            .split(',')
            .filter_map(|pair| {
                let parts: Vec<&str> = pair.splitn(2, '=').collect();
                if parts.len() == 2 {
                    Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
                } else {
                    None
                }
            })
            .collect()
    }
}
//! Provider average pricing per 1M tokens in USD (low average)
//! anthropic highest, openai next, deepseek/kimi lowest, custom = lowest

pub const PROVIDER_PRICING: &[(&str, f64, f64)] = &[
    ("anthropic", 3.0, 15.0),
    ("openai", 2.5, 10.0),
    ("deepseek", 0.55, 2.19),
    ("kimi", 1.0, 4.0),
    ("custom", 0.55, 2.19),
];

pub fn price_for(provider: &str) -> (f64, f64) {
    let p = provider.to_lowercase();
    for (k, pi, po) in PROVIDER_PRICING {
        if p == *k {
            return (*pi, *po);
        }
    }
    (0.0, 0.0)
}

pub fn estimate_usd(provider: &str, input_tokens: Option<i64>, output_tokens: Option<i64>) -> f64 {
    let (pi, po) = price_for(provider);
    let inp = input_tokens.unwrap_or(0) as f64;
    let out = output_tokens.unwrap_or(0) as f64;
    inp / 1_000_000.0 * pi + out / 1_000_000.0 * po
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn estimate_provider_avg() {
        // anthropic 10k input, 5k output => 10k/1e6*3 +5k/1e6*15 = 0.03+0.075=0.105
        let v = estimate_usd("anthropic", Some(10_000), Some(5_000));
        assert!((v - 0.105).abs() < 1e-9);
        let o = estimate_usd("openai", Some(8_000), Some(4_000));
        assert!((o - (8_000.0/1e6*2.5 + 4_000.0/1e6*10.0)).abs() < 1e-9);
        let d = estimate_usd("custom", Some(5_000), Some(2_000));
        assert!((d - (5_000.0/1e6*0.55 + 2_000.0/1e6*2.19)).abs() < 1e-9);
    }
}

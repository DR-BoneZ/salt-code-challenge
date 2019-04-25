#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;

use bigdecimal::BigDecimal;
use bigdecimal::Zero;
use failure::Error;
use std::collections::*;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradingPair {
    pub exchange: String,
    pub quote_asset: String,
    pub base_asset: String,
    pub rate: BigDecimal,
    pub capacity: BigDecimal,
}

#[derive(Clone)]
pub struct TradingPairSorting<'a> {
    by_rate: Vec<(usize, &'a TradingPair)>,
    by_capacity: Vec<(usize, &'a TradingPair)>,
}
impl<'a> Default for TradingPairSorting<'a> {
    fn default() -> Self {
        TradingPairSorting {
            by_rate: vec![],
            by_capacity: vec![],
        }
    }
}

#[derive(Clone)]
pub struct TradingPairMap<'a> {
    pub by_base_asset: HashMap<&'a str, TradingPairSorting<'a>>,
    pub by_quote_asset: HashMap<&'a str, TradingPairSorting<'a>>,
}
impl<'a> TradingPairMap<'a> {
    pub fn new() -> Self {
        TradingPairMap {
            by_base_asset: HashMap::new(),
            by_quote_asset: HashMap::new(),
        }
    }

    pub fn from_vec(v: &'a Vec<TradingPair>) -> Self {
        let mut ret = TradingPairMap {
            by_base_asset: HashMap::new(),
            by_quote_asset: HashMap::new(),
        };
        for p in v.iter().enumerate() {
            match ret.by_base_asset.get_mut(&p.1.base_asset.as_str()) {
                None => {
                    ret.by_base_asset.insert(
                        &p.1.base_asset,
                        TradingPairSorting {
                            by_rate: vec![p],
                            by_capacity: vec![p],
                        },
                    );
                }
                Some(ref mut s) => {
                    s.by_rate.push(p);
                    s.by_rate.sort_unstable_by(|a, b| a.1.rate.cmp(&b.1.rate));
                    s.by_capacity.push(p);
                    s.by_capacity
                        .sort_unstable_by(|a, b| a.1.capacity.cmp(&b.1.capacity));
                }
            }
        }
        for p in v.iter().enumerate() {
            match ret.by_quote_asset.get_mut(&p.1.quote_asset.as_str()) {
                None => {
                    ret.by_quote_asset.insert(
                        &p.1.quote_asset,
                        TradingPairSorting {
                            by_rate: vec![p],
                            by_capacity: vec![p],
                        },
                    );
                }
                Some(ref mut s) => {
                    s.by_rate.push(p);
                    s.by_rate.sort_unstable_by(|a, b| a.1.rate.cmp(&b.1.rate));
                    s.by_capacity.push(p);
                    s.by_capacity
                        .sort_unstable_by(|a, b| a.1.capacity.cmp(&b.1.capacity));
                }
            }
        }
        ret
    }
}

pub fn optimize_rate<'a>(
    pair_map: &TradingPairMap<'a>,
    base_asset: &str,
    quote_asset: &str,
) -> Option<&'a TradingPair> {
    pair_map
        .by_base_asset
        .get(base_asset)
        .and_then(|v| {
            v.by_rate
                .iter()
                .filter(|a| a.1.quote_asset == quote_asset)
                .next()
        })
        .map(|p| p.1)
}

pub fn optimize_rate_multi<'a>(
    pairs: Vec<TradingPair>,
    base_asset: &str,
    quote_asset: &'static str,
    n: usize,
    tp: std::sync::Arc<std::sync::Mutex<threadpool::ThreadPool>>,
) -> Option<(BigDecimal, Vec<TradingPair>)> {
    if n == 0 {
        return None;
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let count = pairs
        .clone()
        .into_iter()
        .filter(|p| p.base_asset == base_asset)
        .map(|p| {
            let tx = tx.clone();
            let tp1 = tp.clone();
            let tp2 = tp.clone();
            let pairs = pairs.clone();
            tp1.lock().unwrap().execute(move || {
                let multi = optimize_rate_multi(pairs, &p.quote_asset, quote_asset, n - 1, tp2);
                if p.quote_asset == quote_asset
                    && (multi.is_none() || p.rate < p.rate.clone() * multi.clone().unwrap().0)
                {
                    tx.send(Some((p.rate.clone(), vec![p]))).unwrap();
                } else {
                    if let Some(mut multi) = multi {
                        multi.1.push(p.clone());
                        tx.send(Some((p.rate.clone() * multi.0, multi.1))).unwrap();
                    } else {
                        tx.send(None).unwrap();
                    }
                }
            });
        })
        .count();
    rx.iter().take(count).filter_map(|a| a).fold(
        None,
        |best: Option<(BigDecimal, Vec<TradingPair>)>, (rate, vec)| {
            if best.is_none() || rate < best.clone().unwrap().0 {
                Some((rate, vec))
            } else {
                best
            }
        },
    )
}

lazy_static! {
    static ref BASE_PROGRAM: String = {
        use std::io::Read;
        let mut s = String::new();
        std::fs::File::open("src/base.lp")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        s
    };
}

pub fn optimize_net<'a>(
    pairs: &'a [TradingPair],
    base_asset: &str,
    quote_asset: &str,
    n: usize,
    amount: BigDecimal,
    precision: i32,
) -> (BigDecimal, Vec<&'a TradingPair>) {
    type Frac = fraction::GenericFraction<i32>;

    let mut ctl = clingo::Control::new(vec![]).unwrap();

    ctl.add("base", &[], &*BASE_PROGRAM).unwrap();
    ctl.add(
        "base",
        &[],
        &format!(
            "starting_amount({}, {}).",
            base_asset.to_ascii_lowercase(),
            format!("{}", amount * BigDecimal::from(precision))
                .parse::<f64>()
                .unwrap() as i32
        ),
    )
    .unwrap_or_default();
    for (i, p) in pairs.iter().enumerate() {
        let rate = Frac::from_decimal_str(&format!("{}", p.rate)).unwrap();
        ctl.add(
            "base",
            &[],
            &format!(
                "pair({}, {}, {}, {}, {}, {}).",
                i,
                p.base_asset.to_ascii_lowercase(),
                p.quote_asset.to_ascii_lowercase(),
                rate.numer().unwrap(),
                rate.denom().unwrap(),
                format!("{}", p.capacity.clone() * BigDecimal::from(precision))
                    .parse::<f64>()
                    .unwrap() as i32
            ),
        )
        .unwrap();
    }
    ctl.add("base", &[], &format!("time(0..{}).", n)).unwrap();
    ctl.add(
        "base",
        &[],
        &format!(
            "final_amount(A) :- amount({}, {}, A).",
            n,
            quote_asset.to_ascii_lowercase()
        ),
    )
    .unwrap();
    ctl.ground(&[clingo::Part::new("base", &[]).unwrap()])
        .unwrap();
    let mut handle = ctl.solve(clingo::SolveMode::YIELD, &[]).unwrap();
    let mut sol = None;
    loop {
        handle.resume().unwrap();
        let model = handle.model().unwrap();
        match model {
            Some(m) => sol = Some(m.symbols(clingo::ShowType::SHOWN).unwrap()),
            _ => break,
        }
    }
    {}
    if let Some(model) = sol {
        let mut ret_pairs = Vec::new();
        let mut ret_amount = BigDecimal::zero();
        for sym in model {
            match sym.name().unwrap() {
                "final_amount" => {
                    ret_amount = BigDecimal::from(sym.arguments().unwrap()[0].number().unwrap())
                        / BigDecimal::from(precision);
                }
                "trade" => {
                    ret_pairs.push(&pairs[sym.arguments().unwrap()[1].number().unwrap() as usize])
                }
                _ => (),
            }
        }
        (ret_amount, ret_pairs)
    } else {
        (BigDecimal::zero(), vec![])
    }
}

#[cfg(test)]
mod tests {

    use crate::*;
    use failure::Error;
    use std::str::FromStr;

    #[test]
    fn test_optimize_rate() -> Result<(), Error> {
        let pairs: Vec<TradingPair> =
            serde_json::from_reader(std::fs::File::open("testData.json")?)?;
        let pair_map = TradingPairMap::from_vec(&pairs);
        assert!(
            optimize_rate(&pair_map, "USD", "BTC")
                .ok_or(format_err!("no rates for that pair"))?
                .rate
                == BigDecimal::from_str("4000")?
        );

        Ok(())
    }

    #[test]
    fn test_optimize_rate_multi() -> Result<(), Error> {
        let tp = std::sync::Arc::new(std::sync::Mutex::new(threadpool::ThreadPool::new(16)));
        let pairs: Vec<TradingPair> =
            serde_json::from_reader(std::fs::File::open("testData.json")?)?;
        assert!(optimize_rate_multi(pairs.clone(), "USD", "BTC", 0, tp.clone()).is_none());
        assert!(
            optimize_rate_multi(pairs.clone(), "USD", "BTC", 1, tp.clone())
                .unwrap()
                .0
                == BigDecimal::from_str("4000")?
        );
        let f = optimize_rate_multi(pairs.clone(), "USD", "BTC", 3, tp.clone()).unwrap();
        println!("{:?}", f);
        assert!(f.0 == BigDecimal::from_str("3000")?);

        Ok(())
    }

    #[test]
    fn test_optimize_net() -> Result<(), Error> {
        let pairs: Vec<TradingPair> =
            serde_json::from_reader(std::fs::File::open("testData.json")?)?;
        let f = optimize_net(
            &pairs,
            "USD",
            "BTC",
            5,
            BigDecimal::from(8000),
            std::u16::MAX as i32,
        );
        println!("BTC: {}", f.0);
        // assert!(f.0 == BigDecimal::from_str("2.5")?);

        Ok(())
    }
}

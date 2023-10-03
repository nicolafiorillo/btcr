use super::tx_out::TxOut;
use std::fmt::{Display, Formatter};

use std::ops::Index;

#[derive(Debug, Clone)]
pub struct TxOuts(Vec<TxOut>);

impl TxOuts {
    pub fn new(txs_out: Vec<TxOut>) -> Self {
        TxOuts(txs_out)
    }

    pub fn amount(&self) -> u64 {
        self.0.iter().fold(0u64, |acc, i: &TxOut| acc + i.amount)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn serialize(&self) -> Vec<u8> {
        self.0.iter().flat_map(|i| i.serialize()).collect()
    }
}

impl Index<usize> for TxOuts {
    type Output = TxOut;

    fn index(&self, index: usize) -> &TxOut {
        &self.0[index]
    }
}

impl Display for TxOuts {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for tx_out in &self.0 {
            writeln!(
                f,
                "   amount: {:}\n   script_pub_key: {:}",
                tx_out.amount, tx_out.script_pub_key,
            )?
        }
        writeln!(f)
    }
}
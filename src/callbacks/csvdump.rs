use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use clap::{Arg, ArgMatches, Command};

use crate::blockchain::proto::block::Block;
use crate::blockchain::proto::tx::{EvaluatedTx, EvaluatedTxOut, TxInput};
use crate::blockchain::proto::Hashed;
use crate::callbacks::Callback;
use crate::common::utils;
use crate::errors::OpResult;

/// Dumps the whole blockchain into csv files
pub struct CsvDump {
    // Each structure gets stored in a separate csv file
    dump_folder: PathBuf,
    block_writer: BufWriter<File>,
    tx_writer: BufWriter<File>,
    txin_writer: BufWriter<File>,
    txout_writer: BufWriter<File>,

    start_height: u64,
    tx_count: u64,
    in_count: u64,
    out_count: u64,
}

impl CsvDump {
    fn create_writer(cap: usize, path: PathBuf) -> OpResult<BufWriter<File>> {
        Ok(BufWriter::with_capacity(cap, File::create(path)?))
    }
}

impl Callback for CsvDump {
    fn build_subcommand() -> Command
    where
        Self: Sized,
    {
        Command::new("csvdump")
            .about("Dumps the whole blockchain into CSV files")
            .version("0.1")
            .author("gcarq <egger.m@protonmail.com>")
            .arg(
                Arg::new("dump-folder")
                    .help("Folder to store csv files")
                    .index(1)
                    .required(true),
            )
    }

    fn new(matches: &ArgMatches) -> OpResult<Self>
    where
        Self: Sized,
    {
        let dump_folder = &PathBuf::from(matches.get_one::<String>("dump-folder").unwrap());
        let cap = 4000000;
        let cb = CsvDump {
            dump_folder: PathBuf::from(dump_folder),
            block_writer: CsvDump::create_writer(cap, dump_folder.join("blocks.csv.tmp"))?,
            tx_writer: CsvDump::create_writer(cap, dump_folder.join("transactions.csv.tmp"))?,
            txin_writer: CsvDump::create_writer(cap, dump_folder.join("tx_in.csv.tmp"))?,
            txout_writer: CsvDump::create_writer(cap, dump_folder.join("tx_out.csv.tmp"))?,
            start_height: 0,
            tx_count: 0,
            in_count: 0,
            out_count: 0,
        };
        Ok(cb)
    }

    fn on_start(&mut self, block_height: u64) -> OpResult<()> {
        self.start_height = block_height;
        info!(target: "callback", "Executing csvdump with dump folder: {} ...", &self.dump_folder.display());
        Ok(())
    }

    fn on_block(&mut self, block: &Block, block_height: u64) -> OpResult<()> {
        // serialize block
        self.block_writer
            .write_all(block.as_csv(block_height).as_bytes())?;

        // serialize transaction
        let block_hash = format!("{}", &block.header.hash);
        for tx in &block.txs {
            self.tx_writer
                .write_all(tx.as_csv(&block_hash).as_bytes())?;
            let txid_str = format!("{}", &tx.hash);

            // serialize inputs
            for input in &tx.value.inputs {
                self.txin_writer
                    .write_all(input.as_csv(&txid_str).as_bytes())?;
            }
            self.in_count += tx.value.in_count.value;

            // serialize outputs
            for (i, output) in tx.value.outputs.iter().enumerate() {
                self.txout_writer
                    .write_all(output.as_csv(&txid_str, i as u32).as_bytes())?;
            }
            self.out_count += tx.value.out_count.value;
        }
        self.tx_count += block.tx_count.value;
        Ok(())
    }

    fn on_complete(&mut self, block_height: u64) -> OpResult<()> {
        // Keep in sync with c'tor
        for f in ["blocks", "transactions", "tx_in", "tx_out"] {
            // Rename temp files
            fs::rename(
                self.dump_folder.as_path().join(format!("{}.csv.tmp", f)),
                self.dump_folder
                    .as_path()
                    .join(format!("{}-{}-{}.csv", f, self.start_height, block_height)),
            )?;
        }

        info!(target: "callback", "Done.\nDumped blocks from height {} to {}:\n\
                                   \t-> transactions: {:9}\n\
                                   \t-> inputs:       {:9}\n\
                                   \t-> outputs:      {:9}",
             self.start_height, block_height, self.tx_count, self.in_count, self.out_count);
        Ok(())
    }
}

impl Block {
    fn as_csv(&self, block_height: u64) -> String {
        // (@hash, height, version, blocksize, @hashPrev, @hashMerkleRoot, nTime, nBits, nNonce)
        format!(
            "{};{};{};{};{};{};{};{};{}\n",
            &self.header.hash,
            &block_height,
            &self.header.value.version,
            &self.size,
            &self.header.value.prev_hash,
            &self.header.value.merkle_root,
            &self.header.value.timestamp,
            &self.header.value.bits,
            &self.header.value.nonce
        )
    }
}

impl Hashed<EvaluatedTx> {
    fn as_csv(&self, block_hash: &str) -> String {
        // (@txid, @hashBlock, version, lockTime)
        format!(
            "{};{};{};{}\n",
            &self.hash, &block_hash, &self.value.version, &self.value.locktime
        )
    }
}

impl TxInput {
    fn as_csv(&self, txid: &str) -> String {
        // (@txid, @hashPrevOut, indexPrevOut, scriptSig, sequence)
        format!(
            "{};{};{};{};{}\n",
            &txid,
            &self.outpoint.txid,
            &self.outpoint.index,
            &utils::arr_to_hex(&self.script_sig),
            &self.seq_no
        )
    }
}

impl EvaluatedTxOut {
    fn as_csv(&self, txid: &str, index: u32) -> String {
        let address = match self.script.address.clone() {
            Some(address) => address,
            None => {
                debug!(target: "csvdump", "Unable to evaluate address for utxo in txid: {} ({})", txid, self.script.pattern);
                String::new()
            }
        };

        // (@txid, indexOut, value, @scriptPubKey, address)
        format!(
            "{};{};{};{};{}\n",
            &txid,
            &index,
            &self.out.value,
            &utils::arr_to_hex(&self.out.script_pubkey),
            &address
        )
    }
}

use crate::config;
use anyhow::{anyhow, Error, Result};
use ethcontract::DynTransport;
use futures::compat::Future01CompatExt;
use std::collections::HashMap;
use std::rc::Rc;
use web3::error::Error as Web3Error;
use web3::types::{Address, U256};
use web3::Transport;

#[derive(Clone, Debug)]
pub struct BalanceMonitor {
    web3: web3::Web3<DynTransport>,
    addresses: Vec<AddressToMonitor>,
}

impl BalanceMonitor {
    pub fn new(config: config::Config, web3: web3::Web3<DynTransport>) -> Result<Self> {
        let tokens = create_tokens(config.tokens, &web3);
        let addresses = create_addresses_to_monitor(config.addresses, &tokens)?;
        Ok(Self { web3, addresses })
    }

    /// Retrieve all balances and call a function for each.
    pub async fn do_with_balances<T>(&self, callback: T)
    where
        T: Fn(
            /*address_name: */ &str,
            /*token_name: */ &str,
            /*balance: */ Result<U256>,
        ),
    {
        // TODO: batch requests
        for address in &self.addresses {
            if address.monitor_ether {
                let balance = ether_balance(address.address, &self.web3.eth()).await;
                callback(&address.name, "ether", balance.map_err(Error::new));
            }
            for token in &address.tokens {
                let balance = erc20_balance(&token.contract, address.address).await;
                callback(&address.name, &token.name, balance.map_err(Error::new));
            }
        }
    }
}

ethcontract::contract!("contracts/IERC20.json");

#[derive(Clone, Debug)]
struct Token {
    name: String,
    contract: IERC20,
}

#[derive(Clone, Debug)]
struct AddressToMonitor {
    name: String,
    address: Address,
    monitor_ether: bool,
    tokens: Vec<Rc<Token>>,
}

fn create_tokens(
    tokens: HashMap<String, config::Address_>,
    web3: &web3::Web3<DynTransport>,
) -> HashMap<String, Rc<Token>> {
    tokens
        .into_iter()
        .map(|(name, address)| {
            (
                name.clone(),
                Rc::new(Token {
                    name,
                    contract: IERC20::at(web3, address.0),
                }),
            )
        })
        .collect()
}

fn create_addresses_to_monitor(
    addresses: HashMap<String, config::ConfigAddress>,
    tokens: &HashMap<String, Rc<Token>>,
) -> Result<Vec<AddressToMonitor>> {
    addresses
        .into_iter()
        .map(|(name, config_address)| {
            let tokens: Result<Vec<Rc<Token>>> = config_address
                .tokens
                .iter()
                .map(|name| {
                    tokens
                        .get(name)
                        .ok_or_else(|| anyhow!("token named {} not found", name))
                        .map(|token| token.clone())
                })
                .collect();
            Ok(AddressToMonitor {
                name,
                address: config_address.address.0,
                monitor_ether: config_address.ether,
                tokens: tokens?,
            })
        })
        .collect()
}

async fn ether_balance(
    address: Address,
    eth_api: &web3::api::Eth<impl Transport>,
) -> Result<U256, Web3Error> {
    eth_api.balance(address, None).compat().await
}

async fn erc20_balance(
    contract: &IERC20,
    address: Address,
) -> Result<U256, ethcontract::errors::MethodError> {
    contract.balance_of(address).call().await
}
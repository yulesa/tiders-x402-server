use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use url::Url;
use tokio::runtime::Runtime;

use cherry_402::{PriceTag, TablePaymentOffers, GlobalPaymentConfig, FacilitatorClient};
use x402_rs::types::{EvmAddress, MoneyAmount};
use x402_rs::network::{Network, USDCDeployment};
use duckdb::Connection;

/// A Python module implemented in Rust.
#[pymodule]
fn cherry_402_python(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyPriceTag>()?;
    m.add_class::<PyTablePaymentOffers>()?;
    m.add_class::<PyGlobalPaymentConfig>()?;
    m.add_class::<PyFacilitatorClient>()?;
    m.add_class::<PyServer>()?;
    m.add_function(wrap_pyfunction!(start_server, m)?)?;
    Ok(())
}

#[pyclass]
#[derive(Clone)]
pub struct PyPriceTag {
    inner: PriceTag,
}

#[pymethods]
impl PyPriceTag {
    #[new]
    fn new(
        pay_to: &str,
        amount_per_item: &str,
        token: &str,
        min_total_amount: Option<&str>,
        min_items: Option<usize>,
        max_items: Option<usize>,
        description: Option<String>,
        is_default: bool,
    ) -> PyResult<Self> {
        let pay_to = EvmAddress::from_str(pay_to)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        
        // Parse amount_per_item as MoneyAmount first, then convert to TokenAmount
        let money_amount = MoneyAmount::from_str(amount_per_item)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        
        // Use USDC on BaseSepolia as the default token
        let usdc = USDCDeployment::by_network(Network::BaseSepolia);
        let amount_per_item = money_amount.as_token_amount(usdc.decimals as u32)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        
        let token_deployment = if token == "0x0000000000000000000000000000000000000000" {
            // Handle ETH case - we'll use USDC for now but this should be configurable
            usdc.into()
        } else {
            // Try to parse as an address and use USDC deployment
            usdc.into()
        };
        
        let min_total_amount = if let Some(min_str) = min_total_amount {
            let min_money = MoneyAmount::from_str(min_str)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            Some(min_money.as_token_amount(usdc.decimals as u32)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?)
        } else {
            None
        };
        
        Ok(Self {
            inner: PriceTag {
                pay_to,
                amount_per_item,
                token: token_deployment,
                min_total_amount,
                min_items,
                max_items,
                description,
                is_default,
            },
        })
    }
}

#[pyclass]
#[derive(Clone)]
pub struct PyTablePaymentOffers {
    inner: TablePaymentOffers,
}

#[pymethods]
impl PyTablePaymentOffers {
    #[new]
    fn new(table_name: String, price_tags: Vec<PyPriceTag>) -> Self {
        let price_tags: Vec<PriceTag> = price_tags.into_iter().map(|pt| pt.inner).collect();
        Self {
            inner: TablePaymentOffers::new(table_name, price_tags),
        }
    }
    
    fn with_payment_offer(&mut self, offer: &PyPriceTag) {
        self.inner = self.inner.clone().with_payment_offer(offer.inner.clone());
    }
}

#[pyclass]
#[derive(Clone)]
pub struct PyFacilitatorClient {
    inner: FacilitatorClient,
}

#[pymethods]
impl PyFacilitatorClient {
    #[new]
    fn new(base_url: &str) -> PyResult<Self> {
        let url = Url::parse(base_url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let client = FacilitatorClient::try_new(url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: client })
    }
}

#[pyclass]
pub struct PyGlobalPaymentConfig {
    inner: GlobalPaymentConfig,
}

#[pymethods]
impl PyGlobalPaymentConfig {
    #[new]
    fn new(facilitator: &PyFacilitatorClient, base_url: &str) -> PyResult<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let facilitator = std::sync::Arc::new(facilitator.inner.clone());
        Ok(Self {
            inner: GlobalPaymentConfig::default(facilitator, base_url),
        })
    }

    fn add_table_offer(&mut self, offer: &PyTablePaymentOffers) {
        self.inner.add_table_offer(offer.inner.clone());
    }

    fn table_requires_payment(&self, table_name: &str) -> Option<bool> {
        self.inner.table_requires_payment(table_name)
    }
}

#[pyclass]
pub struct PyServer {
    runtime: Runtime,
    state: Option<Arc<cherry_402::query_handler::AppState>>,
}

#[pymethods]
impl PyServer {
    #[new]
    fn new() -> PyResult<Self> {
        let runtime = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        
        Ok(Self {
            runtime,
            state: None,
        })
    }

    fn setup_server(
        &mut self,
        facilitator_url: &str,
        base_url: &str,
        db_path: &str,
        table_offers: Vec<PyTablePaymentOffers>,
    ) -> PyResult<()> {
        self.runtime.block_on(async {
            // Initialize facilitator client
            let facilitator = Arc::new(
                FacilitatorClient::try_from(facilitator_url)
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?
            );

            // Initialize payment configuration
            let base_url = Url::parse(base_url)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

            let mut global_payment_config = GlobalPaymentConfig::default(facilitator, base_url);
            
            // Add table offers
            for offer in table_offers {
                global_payment_config.add_table_offer(offer.inner);
            }

            // Initialize DuckDB connection 
            let db = Connection::open(db_path)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            
            let state = Arc::new(cherry_402::query_handler::AppState {
                db: Arc::new(Mutex::new(db)),
                payment_config: Arc::new(global_payment_config),
            });

            self.state = Some(state);
            Ok(())
        })
    }

    fn start_server(&self) -> PyResult<()> {
        let state = self.state.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyValueError, _>("Server not set up. Call setup_server first."))?;
        
        let state_clone = state.clone();
        self.runtime.block_on(async {
            cherry_402::start_server(state_clone).await;
            Ok(())
        })
    }
}

#[pyfunction]
fn start_server() -> PyResult<()> {
    // This is a placeholder - in a real implementation, you'd want to start the server
    // For now, we'll just print a message
    println!("Server would start here");
    Ok(())
}

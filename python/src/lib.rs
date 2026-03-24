/// tiders_x402_python: Python bindings for the tiders_x402 Rust library.
///
/// This module exposes payment, server, and configuration primitives for use in Python.
///
/// Exposed classes:
/// - PriceTag: Represents a payment offer for a table or item.
/// - USDCDeployment: Represents a USDC token deployment on a supported network.
/// - TablePaymentOffers: Holds payment offers for a table.
/// - GlobalPaymentConfig: Global configuration for payment and facilitator.
/// - AppState: Application state, including database and payment config.
/// - FacilitatorClient: Client for interacting with a payment facilitator.
/// - Server: Runs a payment-enabled DuckDB server.
use pyo3::prelude::*;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use url::Url;
use tokio::runtime::Runtime;
use duckdb::arrow::datatypes::Schema;

use tiders_x402::{PriceTag, TablePaymentOffers, GlobalPaymentConfig, AppState, FacilitatorClient};
use tiders_x402::price::TokenAmount;
use x402_chain_eip155::chain::{ChecksummedAddress, Eip155TokenDeployment};
use x402_chain_eip155::KnownNetworkEip155;
use x402_types::networks::USDC;
use duckdb::Connection;
use alloy::primitives::U256;
use arrow::pyarrow::{FromPyArrow, ToPyArrow};
use tiders_x402::duckdb_reader::get_duckdb_table_schema;

/// A Python module implemented in Rust.
#[pymodule]
fn tiders_x402_server(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyPriceTag>()?;
    m.add_class::<PyTablePaymentOffers>()?;
    m.add_class::<PyGlobalPaymentConfig>()?;
    m.add_class::<PyAppState>()?;
    m.add_class::<PyUSDCDeployment>()?;
    m.add_class::<PyFacilitatorClient>()?;
    m.add_class::<PyServer>()?;
    m.add_class::<PySchema>()?;
    m.add_function(wrap_pyfunction!(get_duckdb_table_schema_py, m)?)?;
    Ok(())
}

/// Represents a payment offer for a table or item.
#[pyclass(name="PriceTag")] // Rename PyPriceTag in python so the class is called PriceTag
#[derive(Clone)]
pub struct PyPriceTag {
    inner: PriceTag,
}

#[pymethods]
impl PyPriceTag {
    /// Create a new PriceTag.
    ///
    /// Args:
    ///     pay_to (str): EVM address to pay to.
    ///     amount_per_item (Union[str, int]): Amount per item (rows or tuples). If a string (e.g., "0.002" or "$1.23") it is interpreted as a MoneyAmount and converted to a TokenAmount using decimals from the token. If an integer it is interpreted as an amount in the smallest token unit ( without decimals, e.g. 1000000 for 1 USDC).
    ///     token (USDCDeployment): Token with decimals and EIP712 information, currently only USDC is supported.
    ///     min_total_amount (Optional[Union[str, int]]): Minimum total amount for this offer to be valid (optional). Can be a string (e.g., "0.01") or an integer representing the smallest token unit.
    ///     min_items (Optional[int]): Minimum number of items (rows or tuples) for this offer to be valid (optional).
    ///     max_items (Optional[int]): Maximum number of items (rows or tuples) for this offer to be valid (optional).
    ///     description (Optional[str]): Description of the offer (optional).
    ///     is_default (bool): Whether this is the default offer.
    ///
    /// Returns:
    ///     PriceTag: A new PriceTag object.
    #[new]
    fn new(
        pay_to: &str,
        amount_per_item: Py<PyAny>,
        token: &PyUSDCDeployment,
        min_total_amount: Option<Py<PyAny>>,
        min_items: Option<usize>,
        max_items: Option<usize>,
        description: Option<String>,
        is_default: bool,
        py: Python,
    ) -> PyResult<Self> {
        let pay_to = ChecksummedAddress::from_str(pay_to)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

        let token_deployment = &token.inner;

        // Handle amount_per_item as either string or integer
        let amount_per_item = if let Ok(amount_str) = amount_per_item.extract::<String>(py) {
            // Parse as string (MoneyAmount) using token's parse method
            let deployed_amount = token_deployment.parse(amount_str.as_str())
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
            TokenAmount(deployed_amount.amount)
        } else if let Ok(amount_int) = amount_per_item.extract::<i64>(py) {
            // Parse as integer - treat as smallest token unit
            if amount_int < 0 {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Amount cannot be negative"));
            }
            TokenAmount(U256::from(amount_int as u64))
        } else {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "amount_per_item must be either a string (e.g., '0.001') or an integer representing the smallest token unit"
            ));
        };

        let min_total_amount = if let Some(min_obj) = min_total_amount {
            if let Ok(min_str) = min_obj.extract::<String>(py) {
                let deployed_amount = token_deployment.parse(min_str.as_str())
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
                Some(TokenAmount(deployed_amount.amount))
            } else if let Ok(min_int) = min_obj.extract::<i64>(py) {
                if min_int < 0 {
                    return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("Min total amount cannot be negative"));
                }
                Some(TokenAmount(U256::from(min_int as u64)))
            } else {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "min_total_amount must be either a string (e.g., '0.01') or an integer representing the smallest token unit"
                ));
            }
        } else {
            None
        };

        Ok(Self {
            inner: PriceTag {
                pay_to,
                amount_per_item,
                token: token_deployment.clone(),
                min_total_amount,
                min_items,
                max_items,
                description,
                is_default,
            },
        })
    }
}

/// Represents a USDC token deployment (address, decimals, EIP712 information) on a supported network.
#[pyclass(name="USDCDeployment")]
#[derive(Clone)]
pub struct PyUSDCDeployment {
    inner: Eip155TokenDeployment,
}

#[pymethods]
impl PyUSDCDeployment {
    /// Automatically create a new USDCDeployment for a given network.
    ///
    /// Args:
    ///     network (str): Network name (e.g., "base_sepolia", "base", "avalanche_fuji", "avalanche", "polygon", "polygon_amoy").
    ///
    /// Returns:
    ///     USDCDeployment: The deployment for the network.
    #[staticmethod]
    fn by_network(py: Python, network: Py<PyAny>) -> PyResult<Self> {
        // Accept either a string or a Python Enum value
        let network_str = if let Ok(s) = network.extract::<String>(py) {
            s
        } else if let Ok(obj) = network.getattr(py, "value") {
            obj.extract::<String>(py)?
        } else {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Expected a string or Network enum value",
            ));
        };

        let deployment = match network_str.as_str() {
            "base_sepolia" => USDC::base_sepolia(),
            "base" => USDC::base(),
            "avalanche_fuji" => USDC::avalanche_fuji(),
            "avalanche" => USDC::avalanche(),
            "polygon" => USDC::polygon(),
            "polygon_amoy" => USDC::polygon_amoy(),
            _ => return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Invalid network: {}. Supported: base_sepolia, base, avalanche_fuji, avalanche, polygon, polygon_amoy", network_str)
            )),
        };

        Ok(Self { inner: deployment })
    }
}

#[pyclass(name="Schema")]
pub struct PySchema {
    pub inner: Schema,
}

#[pymethods]
impl PySchema {
    /// Create a new PySchema from a pyarrow.Schema.
    ///
    /// Args:
    ///     py_schema (pyarrow.Schema): The pyarrow.Schema to convert to a PySchema.
    ///
    /// Returns:
    ///     PySchema: A new PySchema object.
    #[new]
    fn new(py_schema: &Bound<'_, PyAny>) -> PyResult<Self> {
        let schema = Schema::from_pyarrow_bound(py_schema)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: schema })
    }

    /// Convert the PySchema to a pyarrow.Schema.
    ///
    /// Args:
    ///     py (Python): The Python interpreter.
    ///
    /// Returns:
    ///     pyarrow.Schema: The pyarrow.Schema representation of the PySchema.
    fn to_pyarrow<'py>(&self, py: Python<'py>) -> PyResult<Py<PyAny>> {
        let bound = self.inner.to_pyarrow(py)?;
        Ok(bound.unbind())
    }
}


/// Get the schema of a table in a DuckDB database.
///
/// Args:
///     db_path (str): Path to the DuckDB database file.
///     table_name (str): Name of the table to get the schema of.
///
/// Returns:
///     PySchema: The schema of the table.
#[pyfunction]
fn get_duckdb_table_schema_py(db_path: &str, table_name: &str) -> PyResult<PySchema> {
    let db = Connection::open(db_path)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
    let schema = get_duckdb_table_schema(&db, table_name)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
    db.close().map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Error closing database: {:?}", e)))?;
    Ok(PySchema { inner: schema })
}


/// Holds payment offers for a table.
#[pyclass(name="TablePaymentOffers")]
#[derive(Clone)]
pub struct PyTablePaymentOffers {
    inner: TablePaymentOffers,
}

#[pymethods]
impl PyTablePaymentOffers {
    /// Create a new TablePaymentOffers with a table name and price tags.
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///     price_tags (List[PriceTag]): List of price tags for the table.
    ///
    /// Returns:
    ///     TablePaymentOffers: A new TablePaymentOffers object.
    #[new]
    fn new(table_name: String, price_tags: Vec<PyPriceTag>, schema: Option<&PySchema>) -> Self {
        let price_tags: Vec<PriceTag> = price_tags.into_iter().map(|pt| pt.inner).collect();
        let schema_inner = schema.map(|s| s.inner.clone());
        Self {
            inner: TablePaymentOffers::new(table_name, price_tags, schema_inner),
        }
    }

    /// Create a free table (no payment required).
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///
    /// Returns:
    ///     TablePaymentOffers: A new TablePaymentOffers object.
    #[staticmethod]
    fn new_free_table(table_name: String, schema: Option<&PySchema>) -> Self {
        let schema_inner = schema.map(|s| s.inner.clone());
        Self {
            inner: TablePaymentOffers::new_free_table(table_name, schema_inner),
        }
    }

    /// Set a description for the table payment offers.
    ///
    /// Args:
    ///     description (str): Description of the table.
    ///
    /// Returns:
    ///     TablePaymentOffers: The updated TablePaymentOffers object.
    fn with_description(&mut self, description: String) {
        self.inner = self.inner.clone().with_description(description);
    }

    /// Add a payment offer to the table.
    ///
    /// Args:
    ///     offer (PriceTag): The payment offer to add to the table.
    ///
    /// Returns:
    ///     TablePaymentOffers: The updated TablePaymentOffers object with the new payment offer.
    fn with_payment_offer(&mut self, offer: &PyPriceTag) {
        self.inner = self.inner.clone().with_payment_offer(offer.inner.clone());
    }
}

/// Client for interacting with a payment facilitator.
#[pyclass(name="FacilitatorClient")]
#[derive(Clone)]
pub struct PyFacilitatorClient {
    inner: FacilitatorClient,
}

#[pymethods]
impl PyFacilitatorClient {
    /// Create a new FacilitatorClient.
    ///
    /// Args:
    ///     base_url (str): Base URL of the facilitator service.
    /// Returns:
    ///     FacilitatorClient: A client for interacting with the facilitator service.
    #[new]
    fn new(base_url: &str) -> PyResult<Self> {
        let url = Url::parse(base_url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let client = FacilitatorClient::try_new(url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: client })
    }
}

/// Global configuration for payment information and facilitator.
#[pyclass(name="GlobalPaymentConfig")]
pub struct PyGlobalPaymentConfig {
    inner: GlobalPaymentConfig,
}

#[pymethods]
impl PyGlobalPaymentConfig {
    /// Create a new GlobalPaymentConfig.
    ///
    /// Args:
    ///     facilitator (FacilitatorClient): Facilitator client.
    ///     base_url (str): Base URL for the app.
    ///
    /// Returns:
    ///     GlobalPaymentConfig: A new GlobalPaymentConfig object.
    #[new]
    fn new(facilitator: &PyFacilitatorClient, base_url: &str) -> PyResult<Self> {
        let base_url = Url::parse(base_url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let facilitator = std::sync::Arc::new(facilitator.inner.clone());
        Ok(Self {
            inner: GlobalPaymentConfig::default(facilitator, base_url),
        })
    }

    /// Add a table payment offer to the global config.
    ///
    /// Args:
    ///     offer (TablePaymentOffers): The table payment offer to add to the global config.
    ///
    /// Returns:
    ///     GlobalPaymentConfig: The updated GlobalPaymentConfig object with the new table payment offer.
    fn add_offers_table(&mut self, offer: &PyTablePaymentOffers) {
        self.inner.add_offers_table(offer.inner.clone());
    }

    /// Check if a table requires payment.
    ///
    /// Args:
    ///     table_name (str): Name of the table.
    ///
    /// Returns:
    ///     bool: True if the table requires payment, False otherwise.
    fn table_requires_payment(&self, table_name: &str) -> Option<bool> {
        self.inner.table_requires_payment(table_name)
    }
}

/// Application state, object mutually shared between API handlers, including database and payment config.
#[pyclass(name="AppState")]
pub struct PyAppState {
    inner: AppState,
}

#[pymethods]
impl PyAppState {
    /// Create a new AppState.
    ///
    /// Args:
    ///     db_path (str): Path to DuckDB database file.
    ///     payment_config (GlobalPaymentConfig): Global payment config.
    ///
    /// Returns:
    ///     AppState: A new AppState object.
    #[new]
    fn new(db_path: &str, payment_config: &PyGlobalPaymentConfig) -> PyResult<Self> {
        let db = Connection::open(db_path)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let state = AppState {
            db: Arc::new(Mutex::new(db)),
            payment_config: Arc::new(payment_config.inner.clone()),
        };
        Ok(Self { inner: state })
    }
}

/// Runs a payment-enabled DuckDB server.
#[pyclass(name="Server")]
pub struct PyServer {
    runtime: Runtime,
    state: Option<Arc<tiders_x402::AppState>>,
}

#[pymethods]
impl PyServer {
    /// Create a new Server instance.
    ///
    /// Args:
    ///     state (AppState): Application state.
    ///
    /// Returns:
    ///     Server: A new Server object.
    #[new]
    fn new(state: &PyAppState) -> PyResult<Self> {
        let runtime = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        Ok(Self {
            runtime,
            state: Some(Arc::new(state.inner.clone())),
        })
    }

    /// Set up the server with facilitator, base URL, DB path, and offer's tables.
    ///
    /// Args:
    ///     facilitator_url (str): Facilitator service URL.
    ///     base_url (str): Base URL for the app.
    ///     db_path (str): Path to DuckDB database file.
    ///     offers_tables (List[TablePaymentOffers]): List of table payment offers.
    fn setup_server(
        &mut self,
        facilitator_url: &str,
        base_url: &str,
        db_path: &str,
        offers_tables: Vec<PyTablePaymentOffers>,
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

            // Add offer's tables
            for offer in offers_tables {
                global_payment_config.add_offers_table(offer.inner);
            }

            // Initialize DuckDB connection
            let db = Connection::open(db_path)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;

            let state = Arc::new(tiders_x402::AppState {
                db: Arc::new(Mutex::new(db)),
                payment_config: Arc::new(global_payment_config),
            });

            self.state = Some(state);
            Ok(())
        })
    }

    /// Start the server (blocking call).
    fn start_server(&self, base_url: &str) -> PyResult<()> {
        let base_url = Url::parse(base_url)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        let state = self.state.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyValueError, _>("Server not set up. Call setup_server first."))?;

        let state_clone = state.clone();
        self.runtime.block_on(async {
            tiders_x402::start_server(state_clone, base_url).await;
            Ok(())
        })
    }
}

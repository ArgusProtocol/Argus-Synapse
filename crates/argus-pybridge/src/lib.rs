use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use argus_ghostdag::{BlockHash, BlockHeader, DagStore, color_dag, total_order_hashes};
use argus_linearizer::schema::LinearizedBlock;

/// A thread-safe wrapper around the GhostDAG store for Python.
#[pyclass]
pub struct PyDagStore {
    inner: Arc<RwLock<DagStore>>,
}

#[pymethods]
impl PyDagStore {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(DagStore::new())),
        }
    }

    /// Add a genesis block.
    pub fn add_genesis(&self, hash_hex: &str, timestamp: u64) -> PyResult<()> {
        let hash = BlockHash::from_hex(hash_hex)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;
        let header = BlockHeader::genesis(hash, timestamp);
        
        let mut dag = self.inner.write().map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Lock poisoned"))?;
        dag.add_genesis(header)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;
        Ok(())
    }

    /// Add a block with parents.
    pub fn add_block(&self, hash_hex: &str, parents_hex: Vec<String>, timestamp: u64) -> PyResult<()> {
        let hash = BlockHash::from_hex(hash_hex)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?;
        let mut parents = Vec::new();
        for p_hex in parents_hex {
            parents.push(BlockHash::from_hex(&p_hex)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("{}", e)))?);
        }
        
        let header = BlockHeader::new(hash, parents, timestamp);
        let mut dag = self.inner.write().map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Lock poisoned"))?;
        dag.add_block(header)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;
        Ok(())
    }

    /// Run the k-coloring algorithm.
    pub fn color_dag(&self, k: u64) -> PyResult<PyObject> {
        let mut dag = self.inner.write().map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Lock poisoned"))?;
        let result = color_dag(&mut dag, k)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;
        
        Python::with_gil(|py| {
            let dict = PyDict::new(py);
            let blue_set: Vec<String> = result.blue_set.iter().map(|h| h.to_hex()).collect();
            let red_set: Vec<String> = result.red_set.iter().map(|h| h.to_hex()).collect();
            dict.set_item("blue_set", blue_set)?;
            dict.set_item("red_set", red_set)?;
            dict.set_item("k", result.k)?;
            Ok(dict.to_object(py))
        })
    }

    /// Return the linearized (PHANTOM ordered) block sequence.
    pub fn get_linearized_stream(&self) -> PyResult<Vec<String>> {
        let dag = self.inner.read().map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Lock poisoned"))?;
        let order = total_order_hashes(&dag)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;
        
        Ok(order.iter().map(|h| h.to_hex()).collect())
    }

    /// Get the current tips.
    pub fn get_tips(&self) -> PyResult<Vec<String>> {
        let dag = self.inner.read().map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Lock poisoned"))?;
        Ok(dag.tips().iter().map(|h| h.to_hex()).collect())
    }
}

/// The Argus PyBridge module.
#[pymodule]
fn argus_pybridge(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyDagStore>()?;
    Ok(())
}

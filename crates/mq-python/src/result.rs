use crate::value::MQValue;

use pyo3::prelude::*;

#[pyclass]
pub struct MQResult {
    pub values: Vec<MQValue>,
}

#[pymethods]
impl MQResult {
    #[getter]
    pub fn text(&self) -> String {
        self.values().join("\n")
    }

    #[getter]
    pub fn values(&self) -> Vec<String> {
        self.values
            .iter()
            .filter_map(|value| if value.__len__() == 0 { None } else { Some(value.text()) })
            .collect::<Vec<String>>()
    }

    pub fn __len__(&self) -> usize {
        self.values.len()
    }

    pub fn __contains__(&self, value: &MQValue) -> PyResult<bool> {
        Ok(self.values.iter().any(|v| v == value))
    }

    pub fn __getitem__(&self, idx: usize) -> PyResult<MQValue> {
        if idx < self.values.len() {
            Ok(self.values[idx].clone())
        } else {
            Err(pyo3::exceptions::PyIndexError::new_err(format!(
                "Index {} out of range for MQResult with length {}",
                idx,
                self.values.len()
            )))
        }
    }

    fn __repr__(&self) -> String {
        format!("MQResult({} items)", self.values.len())
    }

    fn __str__(&self) -> String {
        self.text()
    }

    fn __eq__(&self, other: &Self) -> bool {
        if self.values.len() != other.values.len() {
            return false;
        }

        self.values.iter().zip(other.values.iter()).all(|(a, b)| a.__eq__(b))
    }

    fn __ne__(&self, other: &Self) -> bool {
        !self.__eq__(other)
    }

    fn __lt__(&self, other: &Self) -> bool {
        if self.values.len() != other.values.len() {
            return self.values.len() < other.values.len();
        }

        self.values.iter().zip(other.values.iter()).all(|(a, b)| a.__lt__(b))
    }

    fn __gt__(&self, other: &Self) -> bool {
        if self.values.len() != other.values.len() {
            return self.values.len() > other.values.len();
        }

        self.values.iter().zip(other.values.iter()).all(|(a, b)| a.__gt__(b))
    }
}

impl From<Vec<MQValue>> for MQResult {
    fn from(values: Vec<MQValue>) -> Self {
        Self { values }
    }
}

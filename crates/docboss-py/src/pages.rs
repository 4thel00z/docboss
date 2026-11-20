//! `Pages` and `Page`, and the `to()` format argument: one dict tagged by
//! `format`, or the same keys as keyword arguments.

use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PySlice, PyString, PyTuple};
use serde_json::{Map, Number, Value};

use docboss_pages::{Format, Output};

use crate::{doc_err, DocbossError};

fn format_err(why: impl std::fmt::Display) -> PyErr {
    DocbossError::new_err(format!("invalid format: {why}"))
}

/// A Python value as JSON, for the serde form of [`Format`].
fn json_value(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if value.is_none() {
        return Ok(Value::Null);
    }
    if let Ok(flag) = value.downcast::<PyBool>() {
        return Ok(Value::Bool(flag.is_true()));
    }
    if value.is_instance_of::<PyInt>() {
        if let Ok(number) = value.extract::<i64>() {
            return Ok(Value::from(number));
        }
        return Ok(Value::from(value.extract::<u64>().map_err(format_err)?));
    }
    if value.is_instance_of::<PyFloat>() {
        let number = value.extract::<f64>()?;
        return Ok(Number::from_f64(number).map_or(Value::Null, Value::Number));
    }
    if let Ok(text) = value.downcast::<PyString>() {
        return Ok(Value::String(text.to_str()?.to_string()));
    }
    if let Ok(dict) = value.downcast::<PyDict>() {
        return json_object(dict);
    }
    if value.is_instance_of::<PyList>() || value.is_instance_of::<PyTuple>() {
        let items: PyResult<Vec<Value>> =
            value.try_iter()?.map(|item| json_value(&item?)).collect();
        return Ok(Value::Array(items?));
    }
    Err(format_err(format!(
        "unsupported value of type {}",
        value.get_type().name()?
    )))
}

fn json_object(dict: &Bound<'_, PyDict>) -> PyResult<Value> {
    let mut map = Map::new();
    for (key, value) in dict.iter() {
        let Ok(key) = key.downcast::<PyString>() else {
            return Err(format_err(format!(
                "keys must be strings, not {}",
                key.get_type().name()?
            )));
        };
        map.insert(key.to_str()?.to_string(), json_value(&value)?);
    }
    Ok(Value::Object(map))
}

/// The format of a `to(spec=None, /, **options)` call: the dict passed, or
/// the keyword arguments, never both.
pub(crate) fn format_arg(
    spec: Option<&Bound<'_, PyAny>>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Format> {
    let options = options.filter(|options| !options.is_empty());
    let dict = match (spec, options) {
        (Some(_), Some(_)) => {
            return Err(format_err(
                "pass a format dict or keyword options, not both",
            ))
        }
        (None, None) => {
            return Err(format_err(
                "missing format; pass format=... or a dict with a format key",
            ))
        }
        (None, Some(options)) => options.clone(),
        (Some(spec), None) => spec.downcast::<PyDict>().cloned().map_err(|_| {
            let kind = spec.get_type().name().map(|name| name.to_string());
            format_err(format!(
                "the format must be a dict, not {}",
                kind.unwrap_or_default()
            ))
        })?,
    };
    serde_json::from_value(json_object(&dict)?).map_err(format_err)
}

pub(crate) fn output_object(py: Python<'_>, output: Output) -> PyResult<PyObject> {
    match output {
        Output::Text(text) => Ok(PyString::new(py, &text).into_any().unbind()),
        Output::Bytes(bytes) => Ok(PyBytes::new(py, &bytes).into_any().unbind()),
    }
}

pub(crate) fn output_list(py: Python<'_>, outputs: Vec<Output>) -> PyResult<PyObject> {
    let objects: PyResult<Vec<PyObject>> = outputs
        .into_iter()
        .map(|output| output_object(py, output))
        .collect();
    Ok(PyList::new(py, objects?)?.into_any().unbind())
}

/// A slice of a sequence as Python takes it: `start`, `stop` and `step`
/// may be absent or negative.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Slice {
    start: Option<isize>,
    stop: Option<isize>,
    step: Option<isize>,
}

impl Slice {
    pub(crate) fn from_py(slice: &Bound<'_, PySlice>) -> PyResult<Slice> {
        let slice = Slice {
            start: slice.getattr("start")?.extract()?,
            stop: slice.getattr("stop")?.extract()?,
            step: slice.getattr("step")?.extract()?,
        };
        if slice.step == Some(0) {
            return Err(PyValueError::new_err("slice step cannot be zero"));
        }
        Ok(slice)
    }

    /// The positions the slice selects from a sequence of `len` items.
    pub(crate) fn positions(self, len: usize) -> Vec<usize> {
        let len = len as isize;
        let step = self.step.unwrap_or(1);
        let (lower, upper) = match step > 0 {
            true => (0, len),
            false => (-1, len - 1),
        };
        let clamp = |bound: Option<isize>, absent: isize| match bound {
            None => absent,
            Some(at) if at < 0 => (at + len).max(lower),
            Some(at) => at.min(upper),
        };
        let start = clamp(self.start, if step > 0 { lower } else { upper });
        let stop = clamp(self.stop, if step > 0 { upper } else { lower });
        std::iter::successors(Some(start), |at| at.checked_add(step))
            .take_while(|&at| if step > 0 { at < stop } else { at > stop })
            .filter_map(|at| usize::try_from(at).ok())
            .collect()
    }
}

/// The position of a sequence index that may count from the end.
pub(crate) fn position(index: isize, len: usize) -> PyResult<usize> {
    let count = len as isize;
    let at = if index < 0 { index + count } else { index };
    if !(0..count).contains(&at) {
        return Err(PyIndexError::new_err(format!(
            "page {index} out of range for {len} pages"
        )));
    }
    Ok(at as usize)
}

/// A selection of a document's laid-out pages: every page from
/// `Document.pages()`, or a slice of them. Indexing gives a `Page`,
/// slicing another `Pages`, and `to()` converts every page selected.
#[pyclass(frozen, sequence, module = "docboss._docboss")]
pub struct Pages {
    pub(crate) inner: docboss_pages::Pages,
}

#[pymethods]
impl Pages {
    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        if let Ok(slice) = key.downcast::<PySlice>() {
            let positions = Slice::from_py(slice)?.positions(self.inner.len());
            let pages = Pages {
                inner: self.inner.pick(positions),
            };
            return Ok(Bound::new(py, pages)?.into_any().unbind());
        }
        let index: isize = key.extract()?;
        let at = position(index, self.inner.len())?;
        let Some(page) = self.inner.get(at) else {
            return Err(PyIndexError::new_err(format!("page {index} out of range")));
        };
        Ok(Bound::new(py, Page { inner: page })?.into_any().unbind())
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let pages: Vec<Page> = self.inner.iter().map(|inner| Page { inner }).collect();
        Ok(PyList::new(py, pages)?.try_iter()?.into_any().unbind())
    }

    /// Converts every selected page: a list of strings for text, Markdown,
    /// HTML and JSON, a list of encoded images for PNG, PPM, BMP and JPEG.
    #[pyo3(signature = (spec=None, /, **options))]
    fn to(
        &self,
        py: Python<'_>,
        spec: Option<&Bound<'_, PyAny>>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let format = format_arg(spec, options)?;
        let inner = self.inner.clone();
        let outputs = py
            .allow_threads(move || inner.to(&format))
            .map_err(doc_err)?;
        output_list(py, outputs)
    }

    fn __repr__(&self) -> String {
        format!("<docboss.Pages {} pages>", self.inner.len())
    }
}

/// One laid-out page.
#[pyclass(frozen, module = "docboss._docboss")]
pub struct Page {
    pub(crate) inner: docboss_pages::Page,
}

#[pymethods]
impl Page {
    /// The page's position in the document, counting from 0.
    #[getter]
    fn index(&self) -> usize {
        self.inner.index()
    }

    /// The number `PAGE` fields show on the page.
    #[getter]
    fn number(&self) -> u32 {
        self.inner.number()
    }

    /// Converts the page: a string for text, Markdown, HTML and JSON, the
    /// encoded image for PNG, PPM, BMP and JPEG. The text formats hold the
    /// blocks that start on the page and the notes they reference first.
    #[pyo3(signature = (spec=None, /, **options))]
    fn to(
        &self,
        py: Python<'_>,
        spec: Option<&Bound<'_, PyAny>>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let format = format_arg(spec, options)?;
        let inner = self.inner.clone();
        let output = py
            .allow_threads(move || inner.to(&format))
            .map_err(doc_err)?;
        output_object(py, output)
    }

    fn __repr__(&self) -> String {
        format!(
            "<docboss.Page index={} number={}>",
            self.inner.index(),
            self.inner.number()
        )
    }
}

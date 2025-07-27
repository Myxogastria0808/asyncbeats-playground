use pyo3::ffi::c_str;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDict};
use std::collections::HashMap;

// 可変長引数の話
fn main() -> PyResult<()> {
    let key1 = "key1";
    let val1 = 1;
    let key2 = "key2";
    let val2 = 2;

    Python::with_gil(|py| {
        let fun: Py<PyAny> = PyModule::from_code(
            py,
            c_str!(
                "def example(*args, **kwargs):
                if args != ():
                    print('called with args', args)
                if kwargs != {}:
                    print('called with kwargs', kwargs)
                if args == () and kwargs == {}:
                    print('called with no arguments')
            "
            ),
            c_str!("example.py"),
            c_str!(""),
        )?
        .getattr("example")?
        .into();

        // call object with PyDict
        let kwargs = [(key1, val1)].into_py_dict(py)?;
        fun.call(py, (), Some(&kwargs))?;

        // pass arguments as Vec
        let kwargs = vec![(key1, val1), (key2, val2)];
        fun.call(py, (), Some(&kwargs.into_py_dict(py)?))?;

        // pass arguments as HashMap
        let mut kwargs = HashMap::<&str, i32>::new();
        kwargs.insert(key1, 1);
        fun.call(py, (), Some(&kwargs.into_py_dict(py)?))?;

        // pass arguments of different types as PyDict
        let kwargs = PyDict::new(py);
        kwargs.set_item(key1, val1)?;
        kwargs.set_item(key2, "string")?;
        fun.call(py, (), Some(&kwargs))?;

        Ok(())
    })
}

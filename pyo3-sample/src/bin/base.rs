use pyo3::prelude::*;
use pyo3::types::PyList;

// def example():
//     x = list()   # create a Python list
//     x.append(1)  # append the integer 1 to it
//     y = x        # create a second reference to the list
//     del x        # delete the original reference

//上記のPythonコードをRustで書き直すと以下のようになる。
fn example<'py>(py: Python<'py>) -> PyResult<()> {
    let list = PyList::empty(py);
    list.append(1)?;
    list.append(2)?;
    list.append(3)?;
    println!("List contents: {list:?}");
    drop(list); // Explicitly drop the list
    Ok(())
}

fn main() -> PyResult<()> {
    Python::with_gil(example)
}

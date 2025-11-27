use magnus::{define_module, function, prelude::*, Error};
use mq_c_api::{mq_create, mq_destroy, mq_eval, mq_free_result, MqResult};
use std::ffi::{CStr, CString};

fn mq_run(code: String, input: String, input_format: String) -> Result<Vec<String>, Error> {
    let engine = unsafe { mq_create() };
    if engine.is_null() {
        return Err(Error::new(
            magnus::exception::runtime_error(),
            "Failed to create mq engine",
        ));
    }

    let c_code = match CString::new(code) {
        Ok(s) => s,
        Err(_) => {
            return Err(Error::new(
                magnus::exception::arg_error(),
                "code contains null byte",
            ))
        }
    };
    let c_input = match CString::new(input) {
        Ok(s) => s,
        Err(_) => {
            return Err(Error::new(
                magnus::exception::arg_error(),
                "input contains null byte",
            ))
        }
    };
    let c_input_format = match CString::new(input_format) {
        Ok(s) => s,
        Err(_) => {
            return Err(Error::new(
                magnus::exception::arg_error(),
                "input_format contains null byte",
            ))
        }
    };

    let result: MqResult = unsafe {
        mq_eval(
            engine,
            c_code.as_ptr(),
            c_input.as_ptr(),
            c_input_format.as_ptr(),
        )
    };

    let output = if result.error_msg.is_null() {
        let mut values = Vec::new();
        if !result.values.is_null() {
            unsafe {
                let slice = std::slice::from_raw_parts(result.values, result.values_len);
                for &s_ptr in slice {
                    if !s_ptr.is_null() {
                        let s = CStr::from_ptr(s_ptr).to_string_lossy().into_owned();
                        values.push(s);
                    }
                }
            }
        }
        Ok(values)
    } else {
        let error_msg = unsafe {
            CStr::from_ptr(result.error_msg)
                .to_string_lossy()
                .into_owned()
        };
        Err(Error::new(
            magnus::exception::runtime_error(),
            error_msg,
        ))
    };

    unsafe {
        mq_free_result(result);
        mq_destroy(engine);
    }

    output
}

#[magnus::init]
fn init() -> Result<(), Error> {
    let module = define_module("Mq")?;
    module.define_singleton_method("run", function!(mq_run, 3))?;
    Ok(())
}

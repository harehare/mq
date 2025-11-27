use jni::objects::{JClass, JObject, JString};
use jni::sys::jobjectArray;
use jni::JNIEnv;
use mq_c_api::{mq_create, mq_destroy, mq_eval, mq_free_result, MqResult};
use std::ffi::{CStr, CString};

#[no_mangle]
pub unsafe extern "system" fn Java_Mq_run<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    code: JString<'local>,
    input: JString<'local>,
    input_format: JString<'local>,
) -> jobjectArray {
    let code: String = env.get_string(&code).unwrap().into();
    let input: String = env.get_string(&input).unwrap().into();
    let input_format: String = env.get_string(&input_format).unwrap().into();

    let engine = mq_create();
    if engine.is_null() {
        env.throw_new("java/lang/RuntimeException", "Failed to create mq engine")
            .unwrap();
        return std::ptr::null_mut();
    }

    let c_code = match CString::new(code) {
        Ok(s) => s,
        Err(_) => {
            env.throw_new(
                "java/lang/IllegalArgumentException",
                "code contains null byte",
            )
            .unwrap();
            return std::ptr::null_mut();
        }
    };
    let c_input = match CString::new(input) {
        Ok(s) => s,
        Err(_) => {
            env.throw_new(
                "java/lang/IllegalArgumentException",
                "input contains null byte",
            )
            .unwrap();
            return std::ptr::null_mut();
        }
    };
    let c_input_format = match CString::new(input_format) {
        Ok(s) => s,
        Err(_) => {
            env.throw_new(
                "java/lang/IllegalArgumentException",
                "input_format contains null byte",
            )
            .unwrap();
            return std::ptr::null_mut();
        }
    };

    let result: MqResult = mq_eval(
        engine,
        c_code.as_ptr(),
        c_input.as_ptr(),
        c_input_format.as_ptr(),
    );

    let output = if result.error_msg.is_null() {
        let mut values = Vec::new();
        if !result.values.is_null() {
            let slice = std::slice::from_raw_parts(result.values, result.values_len);
            for &s_ptr in slice {
                if !s_ptr.is_null() {
                    let s = CStr::from_ptr(s_ptr).to_string_lossy().into_owned();
                    values.push(s);
                }
            }
        }

        let output_array = env
            .new_object_array(values.len() as i32, "java/lang/String", JObject::null())
            .unwrap();

        for (i, s) in values.iter().enumerate() {
            let j_string = env.new_string(s).unwrap();
            env.set_object_array_element(&output_array, i as i32, j_string)
                .unwrap();
        }

        output_array.into_raw()
    } else {
        let error_msg = CStr::from_ptr(result.error_msg)
            .to_string_lossy()
            .into_owned();
        env.throw_new("java/lang/RuntimeException", error_msg)
            .unwrap();
        std::ptr::null_mut()
    };

    mq_free_result(result);
    mq_destroy(engine);

    output
}

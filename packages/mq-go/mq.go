package mqgo

/*
#cgo CFLAGS: -I../../crates/mq-c-api/include -I../../target/debug -I../../target/release
// Assuming the library will be in target/debug or target/release relative to the mq-go package
// For dynamic linking:
// #cgo LDFLAGS: -L../../target/debug -lmq_c_api
// For static linking (example, actual name might vary):
// #cgo LDFLAGS: ../../target/debug/libmq_c_api.a
// The exact LDFLAGS will depend on the build process and library location.
// For now, we might need to adjust this or use environment variables during the actual build.
// Let's assume a common scenario where the library is in a known relative path.
// We will likely need to copy the .so/.a and any .h files to a location CGo can find,
// or set these paths more robustly during the build step.

// If we have a header file for mq_c_api (which is good practice but not created yet):
// // #include "mq_c_api.h"
// For now, directly define the functions as they are in Rust.

// Forward declare C types and functions
typedef void MQEngine;

typedef struct {
    char** values;
    unsigned long long values_len; // Ensure this matches Rust's usize. Using unsigned long long for safety.
    char* error_msg;
} MQResult;

MQEngine* mq_engine_create();
void mq_engine_destroy(MQEngine* engine);
MQResult mq_eval(MQEngine* engine, const char* code, const char* input, const char* input_format);
void mq_free_string(char* s);
void mq_free_result(MQResult result);
*/
import "C"
import (
	"errors"
	"unsafe"
)

// Engine is a wrapper around the mq_lang C engine.
type Engine struct {
	ptr *C.MQEngine
}

// NewEngine creates a new mq-lang engine.
func NewEngine() (*Engine, error) {
	cEngine := C.mq_engine_create()
	if cEngine == nil {
		return nil, errors.New("failed to create mq engine (null pointer returned)")
	}
	return &Engine{ptr: cEngine}, nil
}

// Close destroys the underlying C engine and frees associated resources.
func (e *Engine) Close() {
	if e.ptr != nil {
		C.mq_engine_destroy(e.ptr)
		e.ptr = nil
	}
}

// Eval evaluates the given mq code with the provided input.
// inputFormat can be "text" or "markdown".
func (e *Engine) Eval(code string, input string, inputFormat string) ([]string, error) {
	if e.ptr == nil {
		return nil, errors.New("engine is closed or not initialized")
	}

	cCode := C.CString(code)
	defer C.free(unsafe.Pointer(cCode))

	cInput := C.CString(input)
	defer C.free(unsafe.Pointer(cInput))

	cInputFormat := C.CString(inputFormat)
	defer C.free(unsafe.Pointer(cInputFormat))

	cResult := C.mq_eval(e.ptr, cCode, cInput, cInputFormat)
	// defer C.mq_free_result(cResult) // Error and results must be copied before this

	if cResult.error_msg != nil {
        errMsg := C.GoString(cResult.error_msg)
        C.mq_free_result(cResult) // Free result after copying error
		return nil, errors.New(errMsg)
	}

	var results []string
    cValuesSlice := (*[1 << 30]*C.char)(unsafe.Pointer(cResult.values))[:cResult.values_len:cResult.values_len]

	for i := 0; i < int(cResult.values_len); i++ {
		results = append(results, C.GoString(cValuesSlice[i]))
	}

    C.mq_free_result(cResult); // Free result after copying values

	return results, nil
}

// GetVersion is a placeholder to test CGo linkage.
// We'd need a corresponding C function in mq_c_api.
/*
func GetVersion() string {
	// Assume mq_get_version() returns char*
	// cVersion := C.mq_get_version()
	// defer C.mq_free_string(cVersion)
	// return C.GoString(cVersion)
	return "v0.0.0-cgo"
}
*/

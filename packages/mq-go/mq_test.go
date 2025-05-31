package mqgo

/*
#cgo CFLAGS: -I../../crates/mq-c-api/include -I../../target/debug -I../../target/release
// For dynamic linking (adjust path and library name as needed):
// #cgo LDFLAGS: -L../../target/debug -lmq_c_api
// For static linking (adjust path and library name as needed):
// #cgo LDFLAGS: ../../target/debug/libmq_c_api.a
//
// IMPORTANT: Ensure one of the LDFLAGS lines is uncommented or CGO_LDFLAGS env var is set
// when running `go test`. For example, from `packages/mq-go/`:
// CGO_LDFLAGS="-L../../target/debug -lmq_c_api" go test
// or for release:
// CGO_LDFLAGS="-L../../target/release -lmq_c_api" go test
*/
import "C"
import (
	"reflect"
	"strings"
	"testing"
)

func TestNewEngine(t *testing.T) {
	engine, err := NewEngine()
	if err != nil {
		t.Fatalf("NewEngine() error = %v", err)
	}
	if engine == nil {
		t.Fatalf("NewEngine() returned nil engine")
	}
	if engine.ptr == nil {
		t.Fatalf("NewEngine() returned engine with nil ptr")
	}
	defer engine.Close()
}

func TestEngine_Close(t *testing.T) {
	engine, err := NewEngine()
	if err != nil {
		t.Fatalf("NewEngine() error = %v", err)
	}
	engine.Close()
	if engine.ptr != nil {
		t.Errorf("Engine ptr should be nil after Close(), got %v", engine.ptr)
	}
	// Test double close
	engine.Close()
}

func TestEngine_Eval(t *testing.T) {
	engine, err := NewEngine()
	if err != nil {
		t.Fatalf("NewEngine() error = %v", err)
	}
	defer engine.Close()

	tests := []struct {
		name          string
		code          string
		input         string
		inputFormat   string
		want          []string
		wantErr       bool
		wantErrMsg    string // if wantErr is true, check if error message contains this string
	}{
		{
			name:        "Simple text processing: add exclamation",
			code:        "map(x -> add(x, \"!\"))", // Corrected: ensure "!" is a string literal in MQ
			input:       "hello\nworld",
			inputFormat: "text",
			want:        []string{"hello!", "world!"},
			wantErr:     false,
		},
		{
			name:        "Simple markdown processing: extract paragraph value containing 'important'",
			code:        "filter(x -> contains(x.value, \"important\")) | map(x -> x.value)", // Corrected: "important" as string
			input:       "# Title\nThis is an important paragraph.\n\nThis is not.",
			inputFormat: "markdown",
			want:        []string{"This is an important paragraph."},
			wantErr:     false,
		},
		{
			name:        "Error case: invalid mq syntax",
			code:        "this is invalid syntax(",
			input:       "test",
			inputFormat: "text",
			want:        nil,
			wantErr:     true,
			wantErrMsg:  "Error evaluating query",
		},
		{
			name:        "Empty input, text",
			code:        "map(x -> add(x, \"!\"))", // Corrected: "!" as string
			input:       "",
			inputFormat: "text",
			want:        []string{"!"},
			wantErr:     false,
		},
		{
			name:        "Empty input, markdown",
			code:        "map(x -> x.value)",
			input:       "",
			inputFormat: "markdown",
			want:        []string{},
			wantErr:     false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := engine.Eval(tt.code, tt.input, tt.inputFormat)
			if (err != nil) != tt.wantErr {
				t.Errorf("Engine.Eval() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			if tt.wantErr {
				if err == nil {
					t.Errorf("Engine.Eval() expected an error, but got nil")
				} else if tt.wantErrMsg != "" && !strings.Contains(err.Error(), tt.wantErrMsg) {
					t.Errorf("Engine.Eval() error = %v, wantErrMsg %q", err, tt.wantErrMsg)
				}
				return
			}
			if !reflect.DeepEqual(got, tt.want) {
				t.Errorf("Engine.Eval() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestEngine_Eval_ClosedEngine(t *testing.T) {
	engine, err := NewEngine()
	if err != nil {
		t.Fatalf("NewEngine() error = %v", err)
	}
	engine.Close()

	_, err = engine.Eval("code", "input", "text")
	if err == nil {
		t.Error("Engine.Eval() on closed engine should return an error, got nil")
	} else {
		if !strings.Contains(err.Error(), "engine is closed or not initialized") {
			t.Errorf("Engine.Eval() on closed engine error = %v, want specific message", err)
		}
	}
}

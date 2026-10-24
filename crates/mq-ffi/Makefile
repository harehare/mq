# Makefile for building mq-ffi C tests

# Detect OS
UNAME_S := $(shell uname -s)

# Set library extension based on OS
ifeq ($(UNAME_S),Darwin)
    LIB_EXT = dylib
    LIB_PREFIX = lib
else ifeq ($(UNAME_S),Linux)
    LIB_EXT = so
    LIB_PREFIX = lib
else
    LIB_EXT = dll
    LIB_PREFIX =
endif

# Directories
BUILD_DIR = ../../target/debug
LIB_NAME = mq_ffi
LIB_FILE = $(BUILD_DIR)/$(LIB_PREFIX)$(LIB_NAME).$(LIB_EXT)

# Compiler flags
CC = gcc
CFLAGS = -Wall -Wextra -I.
LDFLAGS = -L$(BUILD_DIR) -l$(LIB_NAME)

# macOS specific flags
ifeq ($(UNAME_S),Darwin)
    LDFLAGS += -Wl,-rpath,$(BUILD_DIR)
else ifeq ($(UNAME_S),Linux)
    LDFLAGS += -Wl,-rpath,$(BUILD_DIR)
endif

.PHONY: all build-rust build-c build-cpp test clean

all: test

# Build Rust library
build-rust:
	@echo "Building Rust library..."
	cargo build

# Build C test
build-c: build-rust
	@echo "Building C test..."
	$(CC) $(CFLAGS) test_mq.c -o test_mq $(LDFLAGS)

# Build C++ test, which checks that the header is usable from C++
build-cpp: build-rust
	@echo "Building C++ test..."
	$(CXX) -std=c++11 $(CFLAGS) test_mq_cpp.cpp -o test_mq_cpp $(LDFLAGS)

# Run test
test: build-c build-cpp
	@echo "Running C test..."
	./test_mq
	@echo "Running C++ test..."
	./test_mq_cpp

# Clean build artifacts
clean:
	@echo "Cleaning..."
	rm -f test_mq test_mq_cpp
	cargo clean

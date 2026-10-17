CXX = g++
.DEFAULT_GOAL := build/kyber-native-color-trial.so
CXXFLAGS = -std=c++23 -O2 -fPIC -fno-gnu-unique -Wall -Wextra -Werror
HYPR_FLAGS := $(shell pkg-config --cflags --libs hyprland)
FRAME_DIR := $(abspath ../test-artifacts/native-cursor-trial)
RUST_TRAIL := ../rust/target/release/libkyber_native_ffi.a

.PHONY: rust-trail
rust-trail:
	cd ../rust && cargo build --release -p kyber-native-ffi

$(RUST_TRAIL): rust-trail

build/kyber-native-color-trial.so: trial.cpp trail.h $(RUST_TRAIL)
	mkdir -p build
	$(CXX) $(CXXFLAGS) -shared -DKYBER_NATIVE_DIR='"$(FRAME_DIR)"' trial.cpp $(RUST_TRAIL) -Wl,--exclude-libs,ALL -o $@.next $(HYPR_FLAGS) -ldl -lpthread -lm
	mv $@.next $@

.PHONY: test
test: $(RUST_TRAIL)
	mkdir -p build
	$(CXX) -std=c++23 -O2 -Wall -Wextra -Werror tests/trail-smoke.cpp $(RUST_TRAIL) -ldl -lpthread -lm -o build/trail-smoke
	build/trail-smoke

#![recursion_limit = "1950"]

use cpp::cpp;

pub trait Bounded {
    const MIN: Self;
    const MAX: Self;
}

macro_rules! impl_bounded {
    ($t:ty, $unsigned:ty) => {
        impl Bounded for $t {
            const MIN: Self = <$t>::MIN;
            const MAX: Self = <$t>::MAX;
        }
    };
}
impl Bounded for bool {
    const MIN: Self = false;
    const MAX: Self = true;
}
impl_bounded!(i8, u8);
impl_bounded!(u8, u8);
impl_bounded!(i16, u16);
impl_bounded!(u16, u16);
impl_bounded!(i32, u32);
impl_bounded!(u32, u32);
impl_bounded!(i64, u64);
impl_bounded!(u64, u64);
// FuzzedDataProvider only supports up to 64-bit integral values

// Inverse of FuzzedDataProvider, accepting values to build a raw byte vector.
pub struct Ifdp {
    integrals: Vec<u8>, // Stores integral values
    bytes: Vec<u8>,     // Stores strings and byte vectors
}

impl Ifdp {
    pub fn new() -> Self {
        Ifdp {
            integrals: Vec::new(),
            bytes: Vec::new(),
        }
    }
    pub fn push_bool(&mut self, value: bool) {
        self.push_integral(value);
    }
    pub fn push_integral<T>(&mut self, value: T)
    where
        T: Into<i128> + Bounded,
    {
        self.push_integral_in_range(value, T::MIN, T::MAX);
    }
    pub fn push_integral_in_range<T>(&mut self, value: T, min: T, max: T)
    where
        T: Into<i128> + Bounded,
    {
        let value = value.into();
        let min = min.into();
        let max = max.into();

        assert!(min <= max);
        assert!(min <= value && value <= max);

        let range = max - min;
        if range == 0 {
            return;
        }
        let num_bits = 128 - range.leading_zeros();
        let num_bytes = (num_bits + 7) / 8;

        let value = (value - min) as u64;
        for i in (0..num_bytes).rev() {
            let byte = (value >> (i * 8)) as u8;
            self.integrals.push(byte);
        }
    }
    pub fn push_bytes(&mut self, data: &[u8]) {
        self.bytes.extend(data);
    }
    pub fn push_str(&mut self, input: &str) {
        self.push_str_u8(input.as_bytes());
    }
    pub fn push_str_u8(&mut self, input: &[u8]) {
        for byte in input.iter() {
            if *byte == b'\\' {
                // Map "\" to "\\"
                self.bytes.push(b'\\');
                self.bytes.push(b'\\');
            } else {
                self.bytes.push(*byte);
            }
        }
        // Terminate string
        self.bytes.push(b'\\');
        self.bytes.push(b'_');
    }
    pub fn retrieve_bytes(&self) -> Vec<u8> {
        let mut result = self.bytes.clone();
        result.extend(self.integrals.iter().rev());
        result
    }
}

#[cfg(test)]
cpp! {{
//===- FuzzedDataProvider.h - Utility header for fuzz targets ---*- C++ -* ===//
//
// Part of the LLVM Project, under the Apache License v2.0 with LLVM Exceptions.
// See https://llvm.org/LICENSE.txt for license information.
// SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception
//
//===----------------------------------------------------------------------===//
// A single header library providing an utility class to break up an array of
// bytes. Whenever run on the same input, provides the same output, as long as
// its methods are called in the same order, with the same arguments.
//===----------------------------------------------------------------------===//

#ifndef LLVM_FUZZER_FUZZED_DATA_PROVIDER_H_
#define LLVM_FUZZER_FUZZED_DATA_PROVIDER_H_

#include <algorithm>
#include <array>
#include <climits>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <initializer_list>
#include <limits>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

// In addition to the comments below, the API is also briefly documented at
// https://github.com/google/fuzzing/blob/master/docs/split-inputs.md#fuzzed-data-provider
class FuzzedDataProvider {
 public:
  // |data| is an array of length |size| that the FuzzedDataProvider wraps to
  // provide more granular access. |data| must outlive the FuzzedDataProvider.
  FuzzedDataProvider(const uint8_t *data, size_t size)
      : data_ptr_(data), remaining_bytes_(size) {}
  ~FuzzedDataProvider() = default;

  // See the implementation below (after the class definition) for more verbose
  // comments for each of the methods.

  // Methods returning std::vector of bytes. These are the most popular choice
  // when splitting fuzzing input into pieces, as every piece is put into a
  // separate buffer (i.e. ASan would catch any under-/overflow) and the memory
  // will be released automatically.
  template <typename T> std::vector<T> ConsumeBytes(size_t num_bytes);
  template <typename T>
  std::vector<T> ConsumeBytesWithTerminator(size_t num_bytes, T terminator = 0);
  template <typename T> std::vector<T> ConsumeRemainingBytes();

  // Methods returning strings. Use only when you need a std::string or a null
  // terminated C-string. Otherwise, prefer the methods returning std::vector.
  std::string ConsumeBytesAsString(size_t num_bytes);
  std::string ConsumeRandomLengthString(size_t max_length);
  std::string ConsumeRandomLengthString();
  std::string ConsumeRemainingBytesAsString();

  // Methods returning integer values.
  template <typename T> T ConsumeIntegral();
  template <typename T> T ConsumeIntegralInRange(T min, T max);

  // Methods returning floating point values.
  template <typename T> T ConsumeFloatingPoint();
  template <typename T> T ConsumeFloatingPointInRange(T min, T max);

  // 0 <= return value <= 1.
  template <typename T> T ConsumeProbability();

  bool ConsumeBool();

  // Returns a value chosen from the given enum.
  template <typename T> T ConsumeEnum();

  // Returns a value from the given array.
  template <typename T, size_t size> T PickValueInArray(const T (&array)[size]);
  template <typename T, size_t size>
  T PickValueInArray(const std::array<T, size> &array);
  template <typename T> T PickValueInArray(std::initializer_list<const T> list);

  // Writes data to the given destination and returns number of bytes written.
  size_t ConsumeData(void *destination, size_t num_bytes);

  // Reports the remaining bytes available for fuzzed input.
  size_t remaining_bytes() { return remaining_bytes_; }

 private:
  FuzzedDataProvider(const FuzzedDataProvider &) = delete;
  FuzzedDataProvider &operator=(const FuzzedDataProvider &) = delete;

  void CopyAndAdvance(void *destination, size_t num_bytes);

  void Advance(size_t num_bytes);

  template <typename T>
  std::vector<T> ConsumeBytes(size_t size, size_t num_bytes);

  template <typename TS, typename TU> TS ConvertUnsignedToSigned(TU value);

  const uint8_t *data_ptr_;
  size_t remaining_bytes_;
};

// Returns a std::vector containing |num_bytes| of input data. If fewer than
// |num_bytes| of data remain, returns a shorter std::vector containing all
// of the data that's left. Can be used with any byte sized type, such as
// char, unsigned char, uint8_t, etc.
template <typename T>
std::vector<T> FuzzedDataProvider::ConsumeBytes(size_t num_bytes) {
  num_bytes = std::min(num_bytes, remaining_bytes_);
  return ConsumeBytes<T>(num_bytes, num_bytes);
}

// Similar to |ConsumeBytes|, but also appends the terminator value at the end
// of the resulting vector. Useful, when a mutable null-terminated C-string is
// needed, for example. But that is a rare case. Better avoid it, if possible,
// and prefer using |ConsumeBytes| or |ConsumeBytesAsString| methods.
template <typename T>
std::vector<T> FuzzedDataProvider::ConsumeBytesWithTerminator(size_t num_bytes,
                                                              T terminator) {
  num_bytes = std::min(num_bytes, remaining_bytes_);
  std::vector<T> result = ConsumeBytes<T>(num_bytes + 1, num_bytes);
  result.back() = terminator;
  return result;
}

// Returns a std::vector containing all remaining bytes of the input data.
template <typename T>
std::vector<T> FuzzedDataProvider::ConsumeRemainingBytes() {
  return ConsumeBytes<T>(remaining_bytes_);
}

// Returns a std::string containing |num_bytes| of input data. Using this and
// |.c_str()| on the resulting string is the best way to get an immutable
// null-terminated C string. If fewer than |num_bytes| of data remain, returns
// a shorter std::string containing all of the data that's left.
inline std::string FuzzedDataProvider::ConsumeBytesAsString(size_t num_bytes) {
  static_assert(sizeof(std::string::value_type) == sizeof(uint8_t),
                "ConsumeBytesAsString cannot convert the data to a string.");

  num_bytes = std::min(num_bytes, remaining_bytes_);
  std::string result(
      reinterpret_cast<const std::string::value_type *>(data_ptr_), num_bytes);
  Advance(num_bytes);
  return result;
}

// Returns a std::string of length from 0 to |max_length|. When it runs out of
// input data, returns what remains of the input. Designed to be more stable
// with respect to a fuzzer inserting characters than just picking a random
// length and then consuming that many bytes with |ConsumeBytes|.
inline std::string
FuzzedDataProvider::ConsumeRandomLengthString(size_t max_length) {
  // Reads bytes from the start of |data_ptr_|. Maps "\\" to "\", and maps "\"
  // followed by anything else to the end of the string. As a result of this
  // logic, a fuzzer can insert characters into the string, and the string
  // will be lengthened to include those new characters, resulting in a more
  // stable fuzzer than picking the length of a string independently from
  // picking its contents.
  std::string result;

  // Reserve the anticipated capacity to prevent several reallocations.
  result.reserve(std::min(max_length, remaining_bytes_));
  for (size_t i = 0; i < max_length && remaining_bytes_ != 0; ++i) {
    char next = ConvertUnsignedToSigned<char>(data_ptr_[0]);
    Advance(1);
    if (next == '\\' && remaining_bytes_ != 0) {
      next = ConvertUnsignedToSigned<char>(data_ptr_[0]);
      Advance(1);
      if (next != '\\')
        break;
    }
    result += next;
  }

  result.shrink_to_fit();
  return result;
}

// Returns a std::string of length from 0 to |remaining_bytes_|.
inline std::string FuzzedDataProvider::ConsumeRandomLengthString() {
  return ConsumeRandomLengthString(remaining_bytes_);
}

// Returns a std::string containing all remaining bytes of the input data.
// Prefer using |ConsumeRemainingBytes| unless you actually need a std::string
// object.
inline std::string FuzzedDataProvider::ConsumeRemainingBytesAsString() {
  return ConsumeBytesAsString(remaining_bytes_);
}

// Returns a number in the range [Type's min, Type's max]. The value might
// not be uniformly distributed in the given range. If there's no input data
// left, always returns |min|.
template <typename T> T FuzzedDataProvider::ConsumeIntegral() {
  return ConsumeIntegralInRange(std::numeric_limits<T>::min(),
                                std::numeric_limits<T>::max());
}

// Returns a number in the range [min, max] by consuming bytes from the
// input data. The value might not be uniformly distributed in the given
// range. If there's no input data left, always returns |min|. |min| must
// be less than or equal to |max|.
template <typename T>
T FuzzedDataProvider::ConsumeIntegralInRange(T min, T max) {
  static_assert(std::is_integral<T>::value, "An integral type is required.");
  static_assert(sizeof(T) <= sizeof(uint64_t), "Unsupported integral type.");

  if (min > max)
    abort();

  // Use the biggest type possible to hold the range and the result.
  uint64_t range = static_cast<uint64_t>(max) - static_cast<uint64_t>(min);
  uint64_t result = 0;
  size_t offset = 0;

  while (offset < sizeof(T) * CHAR_BIT && (range >> offset) > 0 &&
         remaining_bytes_ != 0) {
    // Pull bytes off the end of the seed data. Experimentally, this seems to
    // allow the fuzzer to more easily explore the input space. This makes
    // sense, since it works by modifying inputs that caused new code to run,
    // and this data is often used to encode length of data read by
    // |ConsumeBytes|. Separating out read lengths makes it easier modify the
    // contents of the data that is actually read.
    --remaining_bytes_;
    result = (result << CHAR_BIT) | data_ptr_[remaining_bytes_];
    offset += CHAR_BIT;
  }

  // Avoid division by 0, in case |range + 1| results in overflow.
  if (range != std::numeric_limits<decltype(range)>::max())
    result = result % (range + 1);

  return static_cast<T>(static_cast<uint64_t>(min) + result);
}

// Returns a floating point value in the range [Type's lowest, Type's max] by
// consuming bytes from the input data. If there's no input data left, always
// returns approximately 0.
template <typename T> T FuzzedDataProvider::ConsumeFloatingPoint() {
  return ConsumeFloatingPointInRange<T>(std::numeric_limits<T>::lowest(),
                                        std::numeric_limits<T>::max());
}

// Returns a floating point value in the given range by consuming bytes from
// the input data. If there's no input data left, returns |min|. Note that
// |min| must be less than or equal to |max|.
template <typename T>
T FuzzedDataProvider::ConsumeFloatingPointInRange(T min, T max) {
  if (min > max)
    abort();

  T range = .0;
  T result = min;
  constexpr T zero(.0);
  if (max > zero && min < zero && max > min + std::numeric_limits<T>::max()) {
    // The diff |max - min| would overflow the given floating point type. Use
    // the half of the diff as the range and consume a bool to decide whether
    // the result is in the first of the second part of the diff.
    range = (max / 2.0) - (min / 2.0);
    if (ConsumeBool()) {
      result += range;
    }
  } else {
    range = max - min;
  }

  return result + range * ConsumeProbability<T>();
}

// Returns a floating point number in the range [0.0, 1.0]. If there's no
// input data left, always returns 0.
template <typename T> T FuzzedDataProvider::ConsumeProbability() {
  static_assert(std::is_floating_point<T>::value,
                "A floating point type is required.");

  // Use different integral types for different floating point types in order
  // to provide better density of the resulting values.
  using IntegralType =
      typename std::conditional<(sizeof(T) <= sizeof(uint32_t)), uint32_t,
                                uint64_t>::type;

  T result = static_cast<T>(ConsumeIntegral<IntegralType>());
  result /= static_cast<T>(std::numeric_limits<IntegralType>::max());
  return result;
}

// Reads one byte and returns a bool, or false when no data remains.
inline bool FuzzedDataProvider::ConsumeBool() {
  return 1 & ConsumeIntegral<uint8_t>();
}

// Returns an enum value. The enum must start at 0 and be contiguous. It must
// also contain |kMaxValue| aliased to its largest (inclusive) value. Such as:
// enum class Foo { SomeValue, OtherValue, kMaxValue = OtherValue };
template <typename T> T FuzzedDataProvider::ConsumeEnum() {
  static_assert(std::is_enum<T>::value, "|T| must be an enum type.");
  return static_cast<T>(
      ConsumeIntegralInRange<uint32_t>(0, static_cast<uint32_t>(T::kMaxValue)));
}

// Returns a copy of the value selected from the given fixed-size |array|.
template <typename T, size_t size>
T FuzzedDataProvider::PickValueInArray(const T (&array)[size]) {
  static_assert(size > 0, "The array must be non empty.");
  return array[ConsumeIntegralInRange<size_t>(0, size - 1)];
}

template <typename T, size_t size>
T FuzzedDataProvider::PickValueInArray(const std::array<T, size> &array) {
  static_assert(size > 0, "The array must be non empty.");
  return array[ConsumeIntegralInRange<size_t>(0, size - 1)];
}

template <typename T>
T FuzzedDataProvider::PickValueInArray(std::initializer_list<const T> list) {
  // TODO(Dor1s): switch to static_assert once C++14 is allowed.
  if (!list.size())
    abort();

  return *(list.begin() + ConsumeIntegralInRange<size_t>(0, list.size() - 1));
}

// Writes |num_bytes| of input data to the given destination pointer. If there
// is not enough data left, writes all remaining bytes. Return value is the
// number of bytes written.
// In general, it's better to avoid using this function, but it may be useful
// in cases when it's necessary to fill a certain buffer or object with
// fuzzing data.
inline size_t FuzzedDataProvider::ConsumeData(void *destination,
                                              size_t num_bytes) {
  num_bytes = std::min(num_bytes, remaining_bytes_);
  CopyAndAdvance(destination, num_bytes);
  return num_bytes;
}

// Private methods.
inline void FuzzedDataProvider::CopyAndAdvance(void *destination,
                                               size_t num_bytes) {
  std::memcpy(destination, data_ptr_, num_bytes);
  Advance(num_bytes);
}

inline void FuzzedDataProvider::Advance(size_t num_bytes) {
  if (num_bytes > remaining_bytes_)
    abort();

  data_ptr_ += num_bytes;
  remaining_bytes_ -= num_bytes;
}

template <typename T>
std::vector<T> FuzzedDataProvider::ConsumeBytes(size_t size, size_t num_bytes) {
  static_assert(sizeof(T) == sizeof(uint8_t), "Incompatible data type.");

  // The point of using the size-based constructor below is to increase the
  // odds of having a vector object with capacity being equal to the length.
  // That part is always implementation specific, but at least both libc++ and
  // libstdc++ allocate the requested number of bytes in that constructor,
  // which seems to be a natural choice for other implementations as well.
  // To increase the odds even more, we also call |shrink_to_fit| below.
  std::vector<T> result(size);
  if (size == 0) {
    if (num_bytes != 0)
      abort();
    return result;
  }

  CopyAndAdvance(result.data(), num_bytes);

  // Even though |shrink_to_fit| is also implementation specific, we expect it
  // to provide an additional assurance in case vector's constructor allocated
  // a buffer which is larger than the actual amount of data we put inside it.
  result.shrink_to_fit();
  return result;
}

template <typename TS, typename TU>
TS FuzzedDataProvider::ConvertUnsignedToSigned(TU value) {
  static_assert(sizeof(TS) == sizeof(TU), "Incompatible data types.");
  static_assert(!std::numeric_limits<TU>::is_signed,
                "Source type must be unsigned.");

  // TODO(Dor1s): change to `if constexpr` once C++17 becomes mainstream.
  if (std::numeric_limits<TS>::is_modulo)
    return static_cast<TS>(value);

  // Avoid using implementation-defined unsigned to signed conversions.
  // To learn more, see https://stackoverflow.com/questions/13150449.
  if (value <= std::numeric_limits<TS>::max()) {
    return static_cast<TS>(value);
  } else {
    constexpr auto TS_min = std::numeric_limits<TS>::min();
    return TS_min + static_cast<TS>(value - TS_min);
  }
}

#endif // LLVM_FUZZER_FUZZED_DATA_PROVIDER_H_


#include <iostream>
}}

fn main() {
    let name = std::ffi::CString::new("World").unwrap();
    let name_ptr = name.as_ptr();
    let r = unsafe {
        cpp!([name_ptr as "const char *"] -> u32 as "int32_t" {
            std::cout << "Hello, " << name_ptr << std::endl;
            return 42;
        })
    };
    assert_eq!(r, 42)
}

#[cfg(test)]
mod tests {
    use super::Ifdp;
    use cpp::cpp;

    fn create_fuzzed_data_provider(buffer: &[u8]) -> *mut std::ffi::c_void {
        let b_ptr = buffer.as_ptr();
        let b_sz = buffer.len() as u64;
        unsafe {
            cpp!([b_ptr as "const uint8_t*", b_sz as "uint64_t"] -> *mut std::ffi::c_void as "void*" {
                auto* fdp = new FuzzedDataProvider{b_ptr,b_sz}; // buffer is lifetimebound, fdp is memory-leaked
                return static_cast<void*>(fdp);
            })
        }
    }

    fn consume_bool(fdp_ptr: *mut std::ffi::c_void) -> bool {
        unsafe {
            cpp!([fdp_ptr as "void*"] -> bool as "bool" {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                return fdp->ConsumeBool();
            })
        }
    }

    fn consume_u8(fdp_ptr: *mut std::ffi::c_void) -> u8 {
        unsafe {
            cpp!([fdp_ptr as "void*"] -> u8 as "uint8_t" {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                return fdp->ConsumeIntegral<uint8_t>();
            })
        }
    }

    fn consume_i8(fdp_ptr: *mut std::ffi::c_void) -> i8 {
        unsafe {
            cpp!([fdp_ptr as "void*"] -> i8 as "int8_t" {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                return fdp->ConsumeIntegral<int8_t>();
            })
        }
    }

    fn consume_u16(fdp_ptr: *mut std::ffi::c_void) -> u16 {
        unsafe {
            cpp!([fdp_ptr as "void*"] -> u16 as "uint16_t" {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                return fdp->ConsumeIntegral<uint16_t>();
            })
        }
    }

    fn consume_i16(fdp_ptr: *mut std::ffi::c_void) -> i16 {
        unsafe {
            cpp!([fdp_ptr as "void*"] -> i16 as "int16_t" {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                return fdp->ConsumeIntegral<int16_t>();
            })
        }
    }

    fn consume_u32(fdp_ptr: *mut std::ffi::c_void) -> u32 {
        unsafe {
            cpp!([fdp_ptr as "void*"] -> u32 as "uint32_t" {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                return fdp->ConsumeIntegral<uint32_t>();
            })
        }
    }

    fn consume_i32(fdp_ptr: *mut std::ffi::c_void) -> i32 {
        unsafe {
            cpp!([fdp_ptr as "void*"] -> i32 as "int32_t" {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                return fdp->ConsumeIntegral<int32_t>();
            })
        }
    }

    fn consume_u64(fdp_ptr: *mut std::ffi::c_void) -> u64 {
        unsafe {
            cpp!([fdp_ptr as "void*"] -> u64 as "uint64_t" {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                return fdp->ConsumeIntegral<uint64_t>();
            })
        }
    }

    fn consume_i64(fdp_ptr: *mut std::ffi::c_void) -> i64 {
        unsafe {
            cpp!([fdp_ptr as "void*"] -> i64 as "int64_t" {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                return fdp->ConsumeIntegral<int64_t>();
            })
        }
    }

    fn consume_bytes(fdp_ptr: *mut std::ffi::c_void, num_bytes: usize) -> Vec<u8> {
        let mut b_ptr: u64 = 0;
        let mut b_sz: u64 = 0;

        let b_ptr_ptr = &b_ptr;
        let b_sz_ptr = &b_sz;

        unsafe {
            cpp!([fdp_ptr as "void*", num_bytes as "uint64_t", b_ptr_ptr as "uint64_t*", b_sz_ptr as "uint64_t*"] {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                auto* data{new std::vector<uint8_t>{}}; // leaked
                *data=fdp->ConsumeBytes<uint8_t>(num_bytes);

                *b_ptr_ptr=uint64_t{reinterpret_cast<std::uintptr_t>(data->data())};
                *b_sz_ptr=uint64_t{data->size()};
            })
        }

        unsafe {
            let slice = std::slice::from_raw_parts(b_ptr as *const u8, b_sz as usize);
            slice.to_vec()
        }
    }

    fn consume_str(fdp_ptr: *mut std::ffi::c_void) -> Vec<u8> {
        let mut b_ptr: u64 = 0;
        let mut b_sz: u64 = 0;

        let b_ptr_ptr = &b_ptr;
        let b_sz_ptr = &b_sz;

        unsafe {
            cpp!([fdp_ptr as "void*", b_ptr_ptr as "uint64_t*", b_sz_ptr as "uint64_t*"] {
                auto* fdp = static_cast<FuzzedDataProvider*>(fdp_ptr);
                auto* data{new std::string{}}; // leaked
                *data=fdp->ConsumeRandomLengthString();

                *b_ptr_ptr=uint64_t{reinterpret_cast<std::uintptr_t>(data->data())};
                *b_sz_ptr=uint64_t{data->size()};
            })
        }

        unsafe {
            let slice = std::slice::from_raw_parts(b_ptr as *const u8, b_sz as usize);
            slice.to_vec()
        }
    }

    macro_rules! single_int_macro {
        ($num:expr, $expected_bytes:expr, $consume_fn:ident) => {{
            let mut ifdp = Ifdp::new();
            ifdp.push_integral($num);
            let buffer = ifdp.retrieve_bytes();
            assert_eq!(buffer, $expected_bytes);

            let fdp_ptr = create_fuzzed_data_provider(&buffer);
            let r = $consume_fn(fdp_ptr);
            assert_eq!(r, $num);
        }};
    }

    macro_rules! two_int_macro {
        ($num1:expr,$num2:expr, $expected_bytes:expr, $consume_fn1:ident,$consume_fn2:ident) => {{
            let mut ifdp = Ifdp::new();
            ifdp.push_integral($num1);
            ifdp.push_integral($num2);
            let buffer = ifdp.retrieve_bytes();
            assert_eq!(buffer, $expected_bytes);

            let fdp_ptr = create_fuzzed_data_provider(&buffer);
            let r1 = $consume_fn1(fdp_ptr);
            let r2 = $consume_fn2(fdp_ptr);
            assert_eq!(r1, $num1);
            assert_eq!(r2, $num2);
        }};
    }

    #[test]
    fn test_ifdp_bool() {
        for val in [true, false] {
            let mut ifdp = Ifdp::new();
            ifdp.push_bool(val);
            let buffer = ifdp.retrieve_bytes();
            assert_eq!(buffer, vec![val as u8]);

            let fdp_ptr = create_fuzzed_data_provider(&buffer);
            let r = consume_bool(fdp_ptr);
            assert_eq!(r, val);
        }
    }

    #[test]
    fn test_ifdp() {
        single_int_macro!(true, vec![0x01], consume_bool);
        single_int_macro!(false, vec![0x00], consume_bool);
        single_int_macro!(42u8, vec![0x2A], consume_u8);
        single_int_macro!(-42i8, vec![0x56], consume_i8);
        single_int_macro!(255u8, vec![0xFF], consume_u8);
        single_int_macro!(42u16, vec![0x2A, 0x00], consume_u16);
        single_int_macro!(0x11u16, vec![0x11, 0x00], consume_u16);
        single_int_macro!(i16::MAX, vec![0xFF, 0xFF], consume_i16);
        single_int_macro!(i16::MIN, vec![0x00, 0x00], consume_i16);
        single_int_macro!(0x1122u32, vec![0x22, 0x11, 0x00, 0x00], consume_u32);
        single_int_macro!(123456u32, vec![0x40, 0xE2, 0x01, 0x00], consume_u32);
        single_int_macro!(
            0x112233445566778Au64,
            vec![0x8A, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11],
            consume_u64
        );
        single_int_macro!(
            0x1122334455667Au64,
            vec![0x7A, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00],
            consume_u64
        );
        single_int_macro!(
            i64::MAX,
            vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            consume_i64
        );
        single_int_macro!(
            i64::MIN,
            vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            consume_i64
        );

        two_int_macro!(true, false, vec![0x00, 0x01], consume_bool, consume_bool);
        two_int_macro!(
            false,
            0x11223344u32,
            vec![0x44, 0x33, 0x22, 0x11, 0x00],
            consume_bool,
            consume_u32
        );
        two_int_macro!(
            true,
            -0x11223344i32,
            vec![0xBC, 0xCC, 0xDD, 0x6E, 0x01],
            consume_bool,
            consume_i32
        );
    }

    #[test]
    fn test_print_example() {
        let mut ifdp = Ifdp::new();
        // Insert below

ifdp.push_integral_in_range(1759463566i64, 946684801i64, 4133980799i64, );
ifdp.push_str_u8(&[0x64,0x65,0x73,0x63,0x72,0x69,0x70,0x74,0x6f,0x72,0x70,0x72,0x6f,0x63,0x65,0x73,0x73,0x70,0x73,0x62,0x74,]); // (len=21), Limit: 64
ifdp.push_integral_in_range(13u8, 0u8, 255u8, );
ifdp.push_integral_in_range(13u8, 0u8, 255u8, );
ifdp.push_integral_in_range(1u64, 0u64, 19u64, );
ifdp.push_str_u8(&[
0x70,0x73,0x62,0x74,0xff,0x01,0x00,0x5e,0x02,0x00,0x00,0x00,0x01,0xfa,0xc0,0x53,0xcc,0x51,0x64,0x36,0x3b,0x7a,0xbf,0xe4,0x01,0x41,0xb3,0x56,0xdb,0xa9,0xfb,0xe2,0xcd,0xe2,0x8b,0x71,0x78,0xeb,0x40,0x36,0x3f,0x53,0xc3,0xcc,0x58,0x00,0x00,0x00,0x00,0x00,0xfd,0xff,0xff,0xff,0x01,0xe0,0x0f,0x97,0x00,0x00,0x00,0x00,0x00,0x22,0x51,0x20,0x99,0x02,0x11,0xa6,0x4e,0xe4,0x5b,0xbf,0x2e,0x2b,0xac,0xff,0x0c,0x15,0xf7,0xec,0x7e,0x0f,0x8b,0x83,0x8b,0xcb,0x71,0x5a,0xbb,0x4a,0x57,0xaa,0xe4,0x12,0x5c,0xb5,0x00,0x00,0x00,0x00,0x00,0x01,0x01,0x2b,0x80,0x96,0x98,0x00,0x00,0x00,0x00,0x00,0x22,0x51,0x20,0xe5,0xcb,0xa0,0x4b,0xeb,0xca,0x57,0x09,0xb7,0x39,0x24,0xc4,0xaf,0x4b,0x08,0x55,0x4f,0xda,0x84,0x42,0x73,0x21,0x2e,0xa2,0x5e,0x3b,0x2f,0x58,0xdc,0x09,0xd1,0x55,0x01,0x03,0x04,0x83,0x00,0x00,0x00,0x01,0x08,0x43,0x01,0x41,0x40,0x4c,0xe3,0x24,0x4a,0x3c,0xf3,0x0f,0xee,0xe4,0xd4,0x09,0xf3,0x66,0x02,0x7e,0x1b,0xeb,0x03,0x65,0x75,0xf9,0xa1,0xbd,0x2d,0x95,0x0f,0x66,0x64,0x8b,0x1e,0x0d,0xee,0xf7,0xbb,0x87,0xd4,0xe9,0x23,0xca,0x76,0xdc,0x6f,0x7b,0x65,0xa6,0xfe,0x65,0x4d,0xad,0x12,0x0f,0xe5,0xc3,0xc4,0x9e,0x47,0x0c,0xbf,0x40,0x49,0xda,0x22,0x16,0x83,0x21,0x16,0xb4,0x96,0xbf,0xba,0xe1,0x49,0x87,0x81,0x7c,0x53,0xd5,0x92,0xbe,0x0a,0xa6,0x6c,0x45,0xc7,0xb9,0x44,0x43,0xc1,0xf7,0x45,0x51,0x37,0x3f,0x9c,0xe3,0x4d,0x23,0x46,0x19,0x00,0xff,0xf6,0x34,0x23,0x56,0x00,0x00,0x80,0x00,0x00,0x00,0x80,0x00,0x00,0x00,0x80,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x01,0x17,0x20,0xb4,0x96,0xbf,0xba,0xe1,0x49,0x87,0x81,0x7c,0x53,0xd5,0x92,0xbe,0x0a,0xa6,0x6c,0x45,0xc7,0xb9,0x44,0x43,0xc1,0xf7,0x45,0x51,0x37,0x3f,0x9c,0xe3,0x4d,0x23,0x46,0x00,0x00,
]); // , Limit: 4096
ifdp.push_integral_in_range(29u8, 0u8, 255u8, );
ifdp.push_integral_in_range(0u8, 0u8, 255u8, );
ifdp.push_integral_in_range(39u8, 0u8, 255u8, );
ifdp.push_integral_in_range(0u64, 0u64, 19u64, );
ifdp.push_str_u8(&[
0x74,0x72,0x28,0x5b,0x66,0x66,0x66,0x36,0x33,0x34,0x32,0x33,0x2f,0x38,0x36,0x68,0x2f,0x30,0x68,0x2f,0x30,0x68,0x5d,0x74,0x70,0x75,0x62,0x44,0x44,0x66,0x76,0x70,0x74,0x62,0x31,0x47,0x4e,0x64,0x75,0x34,0x54,0x52,0x55,0x55,0x4e,0x66,0x39,0x62,0x77,0x73,0x46,0x46,0x70,0x48,0x41,0x44,0x41,0x6d,0x77,0x61,0x42,0x33,0x75,0x6f,0x43,0x38,0x75,0x6b,0x76,0x35,0x4a,0x4d,0x73,0x59,0x50,0x7a,0x45,0x63,0x33,0x45,0x36,0x79,0x67,0x34,0x77,0x38,0x74,0x34,0x39,0x57,0x71,0x37,0x6a,0x74,0x4c,0x38,0x4c,0x43,0x33,0x56,0x50,0x6d,0x75,0x68,0x58,0x6f,0x53,0x4a,0x78,0x70,0x61,0x6d,0x57,0x44,0x50,0x66,0x70,0x68,0x41,0x7a,0x33,0x46,0x69,0x6f,0x6b,0x69,0x33,0x57,0x65,0x72,0x77,0x48,0x33,0x59,0x2f,0x30,0x2f,0x30,0x29,0x23,0x63,0x6c,0x38,0x32,0x38,0x30,0x67,0x6d,
]); // , Limit: 4096

        // Insert above
        let buffer = ifdp.retrieve_bytes();
        use std::fs::{self, File};
        use std::io::{self, Write};
        File::create("/tmp/ifdp.out")
            .unwrap()
            .write_all(&buffer)
            .unwrap();
    }

    #[test]
    fn test_ifdp_vec() {
        for extra in [true, false] {
            let mut ifdp = Ifdp::new();
            let data = vec![0xde, 0xad, 0xbe, 0xef];
            ifdp.push_bytes(&data);
            if extra {
                ifdp.push_integral(7u8);
            }
            let buffer = ifdp.retrieve_bytes();
            assert_eq!(buffer[..data.len()], data);

            let fdp_ptr = create_fuzzed_data_provider(&buffer);
            let r = consume_bytes(fdp_ptr, data.len());
            assert_eq!(r, data);
            if extra {
                let r = consume_u8(fdp_ptr);
                assert_eq!(r, 7u8);
            }
        }
    }

    #[test]
    fn test_ifdp_str() {
        for extra in [true, false] {
            let mut ifdp = Ifdp::new();
            let data = vec![0x68, 0x69, 0x5C, 0x5F, 0x68, 0x69, 0x5C, 0x5F];
            ifdp.push_str("hi");
            ifdp.push_str("hi");
            if extra {
                ifdp.push_integral(7u8);
            }
            let buffer = ifdp.retrieve_bytes();
            assert_eq!(buffer[..data.len()], data);

            let fdp_ptr = create_fuzzed_data_provider(&buffer);
            let r = consume_str(fdp_ptr);
            assert_eq!(r, [0x68, 0x69]); // "hi"
            let r = consume_str(fdp_ptr);
            assert_eq!(r, [0x68, 0x69]); // "hi"
            if extra {
                let r = consume_u8(fdp_ptr);
                assert_eq!(r, 7u8);
            }
        }
    }
}

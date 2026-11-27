---
title: Traits with associated constants: sizing things at compile time
summary: How FixedSize gives every storable type a SIZE known to the compiler, how generics use it for array capacities, and what const fn can and cannot compute, against C++ templates and sizeof.
minutes: 9
---
An index page holds an array of entries, and how many fit depends on the entry's size. In C++ that is `sizeof(MappingType)` inside a template. In Rust it is an **associated constant** on a trait, and the compiler knows it at compile time.

## The trait

```rust
pub trait FixedSize: Sized {
    const SIZE: usize;                         // the number of bytes of one value in a page
    fn encode(&self, out: &mut [u8]);          // write exactly SIZE bytes
    fn decode(bytes: &[u8]) -> Self;           // read exactly SIZE bytes
}

impl FixedSize for i32 { const SIZE: usize = 4; /* encode, decode via to_le_bytes */ }
impl FixedSize for Rid { const SIZE: usize = 8; /* ... */ }
```

`T::SIZE` is a *constant of the type*, like `sizeof(T)` but defined by you, not by the compiler's layout: it is the size in the **page format**, which need not equal `size_of::<T>()` (a `Rid` struct might have padding; its encoding does not).

## Composing: a pair of fixed-size values is fixed-size

```rust
impl<A: FixedSize, B: FixedSize> FixedSize for (A, B) {
    const SIZE: usize = A::SIZE + B::SIZE;     // computed at compile time from the two parts
    fn encode(&self, out: &mut [u8]) { let (a, b) = out.split_at_mut(A::SIZE); self.0.encode(a); self.1.encode(b); }
    fn decode(bytes: &[u8]) -> (A, B) { let (a, b) = bytes.split_at(A::SIZE); (A::decode(a), B::decode(b)) }
}
```

So `(GenericKey<8>, Rid)` has `SIZE == 16` with no code written for that case. The C++ equivalent is a `std::pair<KeyType, ValueType>` with `sizeof` and a reinterpret cast: shorter, and it bakes the *compiler's* padding into the file format.

## Const fn and array sizes

```rust
pub const fn array_size(metadata_size: usize, entry_size: usize) -> usize {
    (BUSTUB_PAGE_SIZE - metadata_size) / entry_size
}
const BUCKET_CAP: usize = array_size(HTABLE_BUCKET_PAGE_METADATA_SIZE, <(GenericKey<8>, Rid)>::SIZE);   // 8184 / 16 = 511
```

A `const fn` can be evaluated by the compiler, so the result can size an array or appear in a `const` item. **What it cannot do (today)**: use generic parameters in an array length (`[u8; T::SIZE]` for a generic `T` is not allowed on stable Rust), call non-`const` functions, or allocate. That is why page views compute offsets at run time from `T::SIZE` instead of declaring `[T; N]` fields: the **capacity** is a method, `bytes.len() / T::SIZE`.

| C++ | Rust |
|---|---|
| `sizeof(T)` | `size_of::<T>()` (layout) or `T::SIZE` (your page format) |
| `template <typename K, typename V> class Bucket { std::pair<K,V> array_[]; }` | `struct Bucket<B, K, V>` with `K: FixedSize, V: FixedSize` |
| `constexpr size_t N = (PAGE_SIZE - 8) / sizeof(MappingType);` | `const N: usize = array_size(8, <(K, V)>::SIZE);` |
| `static_assert(sizeof(X) <= PAGE_SIZE)` | `const _: () = assert!(size_of::<X>() <= PAGE_SIZE);` |
| duck-typed templates: errors are pages long | trait bounds: `K: FixedSize` is checked at the call site |

## Why a trait and not a macro or an enum

A **trait bound** says what a generic type must provide, once, and the compiler checks it where the type is used; an error is "the trait `FixedSize` is not implemented for `String`". The alternative, a macro that pastes `sizeof`, has no check and no name for the idea. And unlike an enum of known types, a trait is **open**: the B+ tree and the table heap can add their own storable types without touching this module.

> [!NOTE] Limits worth knowing
> `const SIZE` has no default that varies with a generic parameter in an array length (yet), and a trait with an associated `const` is not object safe: you cannot write `&dyn FixedSize`. Neither matters here, because the page views are generic and monomorphised.

## In real code

### The API you will use

| syntax | what it does | when |
|---|---|---|
| `trait T { const SIZE: usize; }` | a constant every implementor must define | sizes, ids, limits |
| `impl T for X { const SIZE: usize = 4; }` | define it | each type |
| `X::SIZE` / `<X as T>::SIZE` / `T::SIZE` (inside `impl<T: Trait>`) | read it, at compile time | generic code |
| `impl<A: T, B: T> T for (A, B) { const SIZE: usize = A::SIZE + B::SIZE; }` | derive a constant from parts | composing sizes |
| `const fn f(a: usize) -> usize { .. }` | a function the compiler can run at compile time | capacities |
| `const _: () = assert!(cond);` | a compile-time assertion | layout facts |
| `PhantomData<T>` | "this struct is generic over `T` but stores none" | typed views over bytes |
| `trait T: Sized` / `where K: FixedSize` | bounds | requiring the capability |

```rust test
trait FixedSize: Sized {
    const SIZE: usize;
    fn encode(&self, out: &mut [u8]);
    fn decode(bytes: &[u8]) -> Self;
}

impl FixedSize for u32 {
    const SIZE: usize = 4;
    fn encode(&self, out: &mut [u8]) { out.copy_from_slice(&self.to_le_bytes()); }
    fn decode(b: &[u8]) -> u32 { u32::from_le_bytes(b.try_into().unwrap()) }
}
impl FixedSize for i64 {
    const SIZE: usize = 8;
    fn encode(&self, out: &mut [u8]) { out.copy_from_slice(&self.to_le_bytes()); }
    fn decode(b: &[u8]) -> i64 { i64::from_le_bytes(b.try_into().unwrap()) }
}
impl<A: FixedSize, B: FixedSize> FixedSize for (A, B) {
    const SIZE: usize = A::SIZE + B::SIZE;
    fn encode(&self, out: &mut [u8]) { let (a, b) = out.split_at_mut(A::SIZE); self.0.encode(a); self.1.encode(b); }
    fn decode(bytes: &[u8]) -> (A, B) { let (a, b) = bytes.split_at(A::SIZE); (A::decode(a), B::decode(b)) }
}

const fn array_size(metadata: usize, entry: usize) -> usize { (8192 - metadata) / entry }
const CAPACITY: usize = array_size(8, <(u32, i64)>::SIZE);
const _: () = assert!(8 + CAPACITY * <(u32, i64)>::SIZE <= 8192);      // the layout fits, or the build fails

#[test]
fn sizes_compose_at_compile_time() {
    assert_eq!(<(u32, i64)>::SIZE, 12);
    assert_eq!(CAPACITY, 682);                                          // 8184 / 12
    let mut buf = [0u8; 12];
    (7u32, -2i64).encode(&mut buf);
    assert_eq!(<(u32, i64)>::decode(&buf), (7, -2));
}
```

```rust test
use std::marker::PhantomData;

trait FixedSize: Sized { const SIZE: usize; fn decode(b: &[u8]) -> Self; }
impl FixedSize for u16 { const SIZE: usize = 2; fn decode(b: &[u8]) -> u16 { u16::from_le_bytes(b.try_into().unwrap()) } }

/// A typed array over any bytes: it stores no `T`, so PhantomData ties the type parameter to the struct.
struct Array<B, T> { bytes: B, _t: PhantomData<T> }

impl<B: AsRef<[u8]>, T: FixedSize> Array<B, T> {
    fn new(bytes: B) -> Self { Array { bytes, _t: PhantomData } }
    fn capacity(&self) -> usize { self.bytes.as_ref().len() / T::SIZE }
    fn get(&self, i: usize) -> T {
        assert!(i < self.capacity(), "entry {i} is past the capacity {}", self.capacity());
        T::decode(&self.bytes.as_ref()[i * T::SIZE..(i + 1) * T::SIZE])
    }
}

#[test]
fn generic_over_the_entry_type() {
    let a: Array<_, u16> = Array::new([1u8, 0, 2, 0, 3, 0]);
    assert_eq!(a.capacity(), 3);
    assert_eq!(a.get(2), 3);
}
```

### In the exercises

- **2a-01:** `FixedSize` for the basic types, `GenericKey<N>`, pairs and the `const fn array_size`.
- **2a-02:** `PageArray<B, T>` is the second example plus `set`, `insert_at` and `remove_at`.
- **2b-02:** a bucket's capacity is computed from `K::SIZE + V::SIZE` of the `FixedSize` bound.

### Where it is used

- **Typed storage layers**: `rkyv`, `zerocopy`, `bincode`'s fixed-size mode and RocksDB's comparator + slice design all separate "how many bytes" from "what value".
- **Embedded and kernel Rust**: `const` sizes and `const _: () = assert!(..)` replace `static_assert` for register maps and ABIs.
- **`std::mem::size_of::<T>()`** is the compiler's version of this; your own `SIZE` is for a *file format*, which must not change when the compiler's layout does.

// Copyright 2014-2016 bluss and ndarray developers.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
//! Experimental BLAS (Basic Linear Algebra Subprograms) integration
//!
//! Depends on crate [`rblas`], ([docs]).
//!
//! [`rblas`]: https://crates.io/crates/rblas/
//! [docs]: http://mikkyang.github.io/rust-blas/doc/rblas/
//!
//! ```
//! extern crate ndarray;
//! extern crate ndarray_rblas;
//! extern crate rblas;
//!
//! use rblas::Gemv;
//! use rblas::attribute::Transpose;
//!
//! use ndarray::{arr1, arr2};
//! use ndarray_rblas::AsBlas;
//!
//! fn main() {
//!     // Gemv is the operation y = α a x + β y
//!     let alpha = 1.;
//!     let mut a = arr2(&[[1., 2., 3.],
//!                        [4., 5., 6.],
//!                        [7., 8., 9.]]);
//!     let x = [1., 0., 1.];
//!     let beta = 1.;
//!     let mut y = arr1(&[0., 0., 0.]);
//!
//!     Gemv::gemv(Transpose::NoTrans, &alpha, &a.blas(), &x[..],
//!                &beta, &mut y.blas());
//!
//!     assert_eq!(y, arr1(&[4., 10., 16.]));
//! }
//!
//! ```
//!
//! Use the methods in trait `AsBlas` to convert an array into a view that
//! implements rblas’ `Vector` or `Matrix` traits.
//!
//! Blas supports strided vectors and matrices; Matrices need to be contiguous
//! in their lowest dimension, so they will be copied into c-contiguous layout
//! automatically if needed. You should be able to use blocks sliced out
//! from a larger matrix without copying. Use the transpose flags in blas
//! instead of transposing with `ndarray`.
//!
//! Blas has its own error reporting system and will not panic on errors (that
//! I know), instead output its own error conditions, for example on dimension
//! mismatch in a matrix multiplication.
//!

extern crate rblas;
extern crate ndarray;

use std::os::raw::{c_int};

use rblas::{
    Matrix,
    Vector,
};
use ndarray::{
    ShapeError,
    ErrorKind,
    ArrayView,
    ArrayViewMut,
    Data,
    DataOwned,
    DataMut,
    Dimension,
    ArrayBase,
    Ix, Ixs,
};


/// ***Requires crate feature `"rblas"`***
pub struct BlasArrayView<'a, A: 'a, D>(ArrayView<'a, A, D>);
impl<'a, A, D: Copy> Copy for BlasArrayView<'a, A, D> { }
impl<'a, A, D: Clone> Clone for BlasArrayView<'a, A, D> {
    fn clone(&self) -> Self {
        BlasArrayView(self.0.clone())
    }
}

/// ***Requires crate feature `"rblas"`***
pub struct BlasArrayViewMut<'a, A: 'a, D>(ArrayViewMut<'a, A, D>);

struct Priv<T>(T);

/// Return `true` if the innermost dimension is contiguous (includes
/// the special cases of 0 or 1 length in that axis).
fn is_inner_contiguous<S, D>(a: &ArrayBase<S, D>) -> bool
    where S: Data,
          D: Dimension,
{
    let ndim = a.ndim();
    if ndim == 0 {
        return true;
    }
    a.shape()[ndim - 1] <= 1 || a.strides()[ndim - 1] == 1
}

/// If the array is not in the standard layout, copy all elements
/// into the standard layout so that the array is C-contiguous.
fn ensure_standard_layout<A, S, D>(a: &mut ArrayBase<S, D>)
    where S: DataOwned<Elem=A>,
          D: Dimension,
          A: Clone
{
    if !a.is_standard_layout() {
        let d = a.dim();
        let v: Vec<A> = a.iter().cloned().collect();
        *a = ArrayBase::from_vec_dim(d, v).unwrap();
    }
}


impl<S, D> Priv<ArrayBase<S, D>>
    where S: Data,
          D: Dimension
{
    fn size_check(&self) -> Result<(), ShapeError> {
        let max = c_int::max_value();
        let self_ = &self.0;
        for (&dim, &stride) in self_.shape().iter().zip(self_.strides()) {
            if dim > max as Ix || stride > max as Ixs {
                return Err(ShapeError::from_kind(ErrorKind::RangeLimited));
            }
        }
        Ok(())
    }

    fn contiguous_check(&self) -> Result<(), ShapeError> {
        // FIXME: handle transposed?
        if is_inner_contiguous(&self.0) {
            Ok(())
        } else {
            Err(ShapeError::from_kind(ErrorKind::IncompatibleLayout))
        }
    }
}

impl<'a, A, D> Priv<ArrayView<'a, A, D>>
    where D: Dimension
{
    pub fn into_blas_view(self) -> Result<BlasArrayView<'a, A, D>, ShapeError> {
        if self.0.ndim() > 1 {
            try!(self.contiguous_check());
        }
        try!(self.size_check());
        Ok(BlasArrayView(self.0))
    }
}

impl<'a, A, D> Priv<ArrayViewMut<'a, A, D>>
    where D: Dimension
{
    fn into_blas_view_mut(self) -> Result<BlasArrayViewMut<'a, A, D>, ShapeError> {
        if self.0.ndim() > 1 {
            try!(self.contiguous_check());
        }
        try!(self.size_check());
        Ok(BlasArrayViewMut(self.0))
    }
}
/*
*/

/// Convert an array into a blas friendly wrapper.
///
/// Note that `blas` suppors four different element types: `f32`, `f64`,
/// `Complex<f32>`, and `Complex<f64>`.
///
/// ***Requires crate feature `"rblas"`***
pub trait AsBlas<A, S, D> {
    /// Return an array view implementing Vector (1D) or Matrix (2D)
    /// traits.
    ///
    /// Elements are copied if needed to produce a contiguous matrix.<br>
    /// The result is always mutable, due to the requirement of having write
    /// access to update the layout either way. Breaks sharing if the array is
    /// an `RcArray`.
    ///
    /// **Errors** if any dimension is larger than `c_int::MAX`.
    fn blas_checked(&mut self) -> Result<BlasArrayViewMut<A, D>, ShapeError>
        where S: DataOwned + DataMut,
              A: Clone;

    /// Equivalent to `.blas_checked().unwrap()`
    ///
    /// **Panics** if there was a an error in `.blas_checked()`.
    fn blas(&mut self) -> BlasArrayViewMut<A, D>
        where S: DataOwned<Elem=A> + DataMut,
              A: Clone
    {
        self.blas_checked().unwrap()
    }

    /// Return a read-only array view implementing Vector (1D) or Matrix (2D)
    /// traits.
    ///
    /// The array must already be in a blas compatible layout: its innermost
    /// dimension must be contiguous.
    ///
    /// **Errors** if any dimension is larger than `c_int::MAX`.<br>
    /// **Errors** if the inner dimension is not c-contiguous.
    ///
    /// Layout requirements may be loosened in the future.
    fn blas_view_checked(&self) -> Result<BlasArrayView<A, D>, ShapeError>
        where S: Data;

    /// `bv` stands for **b**las **v**iew.
    ///
    /// Equivalent to `.blas_view_checked().unwrap()`
    ///
    /// **Panics** if there was a an error in `.blas_view_checked()`.
    fn bv(&self) -> BlasArrayView<A, D>
        where S: Data,
    {
        self.blas_view_checked().unwrap()
    }

    /// Return a read-write array view implementing Vector (1D) or Matrix (2D)
    /// traits.
    ///
    /// The array must already be in a blas compatible layout: its innermost
    /// dimension must be contiguous.
    ///
    /// **Errors** if any dimension is larger than `c_int::MAX`.<br>
    /// **Errors** if the inner dimension is not c-contiguous.
    ///
    /// Layout requirements may be loosened in the future.
    fn blas_view_mut_checked(&mut self) -> Result<BlasArrayViewMut<A, D>, ShapeError>
        where S: DataMut;

    /// `bvm` stands for **b**las **v**iew **m**ut.
    ///
    /// Equivalent to `.blas_view_mut_checked().unwrap()`
    ///
    /// **Panics** if there was a an error in `.blas_view_mut_checked()`.
    fn bvm(&mut self) -> BlasArrayViewMut<A, D>
        where S: DataMut,
    {
        self.blas_view_mut_checked().unwrap()
    }
    /*

    /// Equivalent to `.blas_checked().unwrap()`, except elements
    /// are not copied to make the array contiguous: instead just
    /// dimensions and strides are adjusted, and elements end up in
    /// arbitrary location. Useful if the content of the array doesn't matter.
    ///
    /// **Panics** if there was a an error in `blas_checked`.
    fn blas_overwrite(&mut self) -> BlasArrayViewMut<A, D>
        where S: DataMut;
        */
}

/// ***Requires crate feature `"rblas"`***
impl<A, S, D> AsBlas<A, S, D> for ArrayBase<S, D>
    where S: Data<Elem=A>,
          D: Dimension,
{
    fn blas_checked(&mut self) -> Result<BlasArrayViewMut<A, D>, ShapeError>
        where S: DataOwned + DataMut,
              A: Clone,
    {
        try!(Priv(self.view()).size_check());
        match self.ndim() {
            0 | 1 => { }
            2 => {
                if !is_inner_contiguous(self) {
                    ensure_standard_layout(self);
                }
            }
            _n => ensure_standard_layout(self),
        }
        Priv(self.view_mut()).into_blas_view_mut()
    }

    fn blas_view_checked(&self) -> Result<BlasArrayView<A, D>, ShapeError>
        where S: Data
    {
        Priv(self.view()).into_blas_view()
    }

    fn blas_view_mut_checked(&mut self) -> Result<BlasArrayViewMut<A, D>, ShapeError>
        where S: DataMut,
    {
        Priv(self.view_mut()).into_blas_view_mut()
    }

    /*
    fn blas_overwrite(&mut self) -> BlasArrayViewMut<A, D>
        where S: DataMut,
    {
        self.size_check().unwrap();
        if self.dim.ndim() > 1 {
            self.force_standard_layout();
        }
        BlasArrayViewMut(self.view_mut())
    }
    */
}

/// **Panics** if `as_mut_ptr` is called on a read-only view.
impl<'a, A> Vector<A> for BlasArrayView<'a, A, Ix> {
    fn len(&self) -> c_int {
        self.0.len() as c_int
    }

    fn as_ptr(&self) -> *const A {
        self.0.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut A {
        panic!("ndarray: as_mut_ptr called on BlasArrayView (not mutable)");
    }

    // increment: stride
    fn inc(&self) -> c_int {
        self.0.strides()[0] as c_int
    }
}

impl<'a, A> Vector<A> for BlasArrayViewMut<'a, A, Ix> {
    fn len(&self) -> c_int {
        self.0.len() as c_int
    }

    fn as_ptr(&self) -> *const A {
        self.0.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut A {
        self.0.as_mut_ptr()
    }

    // increment: stride
    fn inc(&self) -> c_int {
        self.0.strides()[0] as c_int
    }
}

/// **Panics** if `as_mut_ptr` is called on a read-only view.
impl<'a, A> Matrix<A> for BlasArrayView<'a, A, (Ix, Ix)> {
    fn rows(&self) -> c_int {
        self.0.dim().0 as c_int
    }

    fn cols(&self) -> c_int {
        self.0.dim().1 as c_int
    }

    // leading dimension == stride between each row
    fn lead_dim(&self) -> c_int {
        debug_assert!(self.cols() <= 1 || self.0.strides()[1] == 1);
        self.0.strides()[0] as c_int
    }

    fn as_ptr(&self) -> *const A {
        self.0.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut A {
        panic!("ndarray: as_mut_ptr called on BlasArrayView (not mutable)");
    }
}

impl<'a, A> Matrix<A> for BlasArrayViewMut<'a, A, (Ix, Ix)> {
    fn rows(&self) -> c_int {
        self.0.dim().0 as c_int
    }

    fn cols(&self) -> c_int {
        self.0.dim().1 as c_int
    }

    // leading dimension == stride between each row
    fn lead_dim(&self) -> c_int {
        debug_assert!(self.cols() <= 1 || self.0.strides()[1] == 1);
        self.0.strides()[0] as c_int
    }

    fn as_ptr(&self) -> *const A {
        self.0.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut A {
        self.0.as_mut_ptr()
    }
}

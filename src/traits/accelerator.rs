//! Accelerator trait definition.
//!
//! This module defines the `Accelerator` trait that abstracts AI accelerators
//! like GPU and TPU in the RISC-V simulator.

use std::fmt::Debug;

/// Accelerator type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorType {
    /// General-purpose GPU for various compute operations
    Gpu,
    /// Tensor Processing Unit optimized for matrix operations
    Tpu,
}

/// Computation precision types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// 16-bit floating point (half precision)
    Fp16 = 0,
    /// 32-bit floating point (single precision)
    Fp32 = 1,
    /// 8-bit signed integer (for quantized operations)
    Int8 = 2,
    /// 32-bit signed integer
    Int32 = 3,
    /// Brain Float 16
    Bf16 = 4,
}

impl Precision {
    /// Get the size in bytes for this precision type.
    pub fn byte_size(&self) -> usize {
        match self {
            Precision::Fp16 | Precision::Bf16 => 2,
            Precision::Fp32 | Precision::Int32 => 4,
            Precision::Int8 => 1,
        }
    }

    /// Create from raw u32 value.
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => Precision::Fp16,
            1 => Precision::Fp32,
            2 => Precision::Int8,
            3 => Precision::Int32,
            4 => Precision::Bf16,
            _ => Precision::Fp32, // Default to FP32
        }
    }
}

/// Kernel types supported by accelerators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelType {
    // Matrix operations
    MatMul = 0,
    MatAdd = 1,
    MatSub = 2,
    MatTranspose = 3,

    // Convolution operations
    Conv2d = 10,
    Conv2dBackward = 11,
    Pool2dMax = 12,
    Pool2dAvg = 13,

    // Vector operations
    VectorAdd = 20,
    VectorMul = 21,
    VectorDot = 22,
    VectorScale = 23,

    // Activation functions
    Relu = 30,
    Relu6 = 31,
    Sigmoid = 32,
    Tanh = 33,
    Softmax = 34,
    LeakyRelu = 35,

    // Reduction operations
    ReduceSum = 40,
    ReduceMax = 41,
    ReduceMean = 42,

    // Quantization
    Quantize = 50,
    Dequantize = 51,
    Requantize = 52,

    // Custom kernel
    Custom = 255,
}

impl KernelType {
    /// Create from raw u32 value.
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => KernelType::MatMul,
            1 => KernelType::MatAdd,
            2 => KernelType::MatSub,
            3 => KernelType::MatTranspose,
            10 => KernelType::Conv2d,
            11 => KernelType::Conv2dBackward,
            12 => KernelType::Pool2dMax,
            13 => KernelType::Pool2dAvg,
            20 => KernelType::VectorAdd,
            21 => KernelType::VectorMul,
            22 => KernelType::VectorDot,
            23 => KernelType::VectorScale,
            30 => KernelType::Relu,
            31 => KernelType::Relu6,
            32 => KernelType::Sigmoid,
            33 => KernelType::Tanh,
            34 => KernelType::Softmax,
            35 => KernelType::LeakyRelu,
            40 => KernelType::ReduceSum,
            41 => KernelType::ReduceMax,
            42 => KernelType::ReduceMean,
            50 => KernelType::Quantize,
            51 => KernelType::Dequantize,
            52 => KernelType::Requantize,
            _ => KernelType::Custom,
        }
    }

    /// Check if this kernel type is a matrix operation.
    pub fn is_matrix_op(&self) -> bool {
        matches!(
            self,
            KernelType::MatMul | KernelType::MatAdd | KernelType::MatSub | KernelType::MatTranspose
        )
    }

    /// Check if this kernel type is a convolution operation.
    pub fn is_conv_op(&self) -> bool {
        matches!(
            self,
            KernelType::Conv2d
                | KernelType::Conv2dBackward
                | KernelType::Pool2dMax
                | KernelType::Pool2dAvg
        )
    }

    /// Check if this kernel type is an activation function.
    pub fn is_activation(&self) -> bool {
        matches!(
            self,
            KernelType::Relu
                | KernelType::Relu6
                | KernelType::Sigmoid
                | KernelType::Tanh
                | KernelType::Softmax
                | KernelType::LeakyRelu
        )
    }
}

/// Tensor descriptor for describing tensor shape and memory layout.
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct TensorDescriptor {
    /// Data address in guest memory (physical address)
    pub data_addr: u64,
    /// Total number of elements
    pub element_count: u32,
    /// Data type (Precision enum value)
    pub precision: u32,
    /// Number of dimensions
    pub ndim: u32,
    /// Shape (max 8 dimensions)
    pub shape: [u32; 8],
    /// Strides (bytes per dimension)
    pub strides: [u32; 8],
    /// Reserved for future use
    pub _reserved: [u32; 4],
}

impl TensorDescriptor {
    /// Create a new tensor descriptor.
    pub fn new(data_addr: u64, shape: &[u32], precision: Precision) -> Self {
        let ndim = shape.len().min(8);
        let mut shape_arr = [0u32; 8];
        let mut strides_arr = [0u32; 8];

        for i in 0..ndim {
            shape_arr[i] = shape[i];
        }

        // Calculate strides (row-major layout)
        let byte_size = precision.byte_size() as u32;
        if ndim > 0 {
            strides_arr[ndim - 1] = byte_size;
            for i in (0..ndim - 1).rev() {
                strides_arr[i] = strides_arr[i + 1] * shape_arr[i + 1];
            }
        }

        let element_count = if ndim > 0 { shape.iter().product() } else { 0 };

        Self {
            data_addr,
            element_count,
            precision: precision as u32,
            ndim: ndim as u32,
            shape: shape_arr,
            strides: strides_arr,
            _reserved: [0; 4],
        }
    }

    /// Get the total byte size of this tensor.
    pub fn byte_size(&self) -> usize {
        let elem_size = Precision::from_u32(self.precision).byte_size();
        self.element_count as usize * elem_size
    }
}

/// Performance counters for accelerators.
#[derive(Debug, Clone, Default)]
pub struct AcceleratorPerfCounters {
    /// Number of kernels executed
    pub kernels_executed: u64,
    /// Total cycles elapsed
    pub cycles_elapsed: u64,
    /// Total bytes transferred via DMA
    pub bytes_transferred: u64,
    /// Total operations performed (FLOPs or INTOPs)
    pub operations_count: u64,
    /// Number of errors encountered
    pub errors_count: u64,
}

/// Accelerator trait for AI accelerators.
///
/// This trait extends the Peripheral trait with accelerator-specific
/// functionality for neural network operations.
pub trait Accelerator: Send + Debug {
    /// Get the accelerator type.
    fn accelerator_type(&self) -> AcceleratorType;

    /// Get the list of supported precision types.
    fn supported_precisions(&self) -> Vec<Precision>;

    /// Get the list of supported kernel types.
    fn supported_kernels(&self) -> Vec<KernelType>;

    /// Get the current performance counters.
    fn performance_counters(&self) -> AcceleratorPerfCounters;

    /// Reset the accelerator to initial state.
    fn reset(&mut self);

    /// Check if the accelerator is currently busy.
    fn is_busy(&self) -> bool;

    /// Get the current kernel type being executed (if any).
    fn current_kernel(&self) -> Option<KernelType>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precision_byte_size() {
        assert_eq!(Precision::Fp16.byte_size(), 2);
        assert_eq!(Precision::Fp32.byte_size(), 4);
        assert_eq!(Precision::Int8.byte_size(), 1);
        assert_eq!(Precision::Int32.byte_size(), 4);
        assert_eq!(Precision::Bf16.byte_size(), 2);
    }

    #[test]
    fn test_precision_from_u32() {
        assert_eq!(Precision::from_u32(0), Precision::Fp16);
        assert_eq!(Precision::from_u32(1), Precision::Fp32);
        assert_eq!(Precision::from_u32(2), Precision::Int8);
        assert_eq!(Precision::from_u32(99), Precision::Fp32); // Default
    }

    #[test]
    fn test_kernel_type_from_u32() {
        assert_eq!(KernelType::from_u32(0), KernelType::MatMul);
        assert_eq!(KernelType::from_u32(10), KernelType::Conv2d);
        assert_eq!(KernelType::from_u32(30), KernelType::Relu);
        assert_eq!(KernelType::from_u32(255), KernelType::Custom);
    }

    #[test]
    fn test_tensor_descriptor() {
        let desc = TensorDescriptor::new(0x8000_0000, &[2, 3], Precision::Fp32);
        assert_eq!(desc.ndim, 2);
        assert_eq!(desc.shape[0], 2);
        assert_eq!(desc.shape[1], 3);
        assert_eq!(desc.element_count, 6);
        assert_eq!(desc.byte_size(), 24); // 6 elements * 4 bytes
    }
}

use crate::abi::call::{Conv, FnAbi, ArgAbi, Reg, RegKind, Uniform};
use crate::abi::{HasDataLayout, LayoutOf, TyLayout, TyLayoutMethods};
use crate::spec::HasTargetSpec;

fn is_homogeneous_aggregate<'a, Ty, C>(cx: &C, arg: &mut ArgAbi<'a, Ty>)
                                     -> Option<Uniform>
    where Ty: TyLayoutMethods<'a, C> + Copy,
          C: LayoutOf<Ty = Ty, TyLayout = TyLayout<'a, Ty>> + HasDataLayout
{
    arg.layout.homogeneous_aggregate(cx).unit().and_then(|unit| {
        let size = arg.layout.pref_pos.size;

        // Ensure we have at most four uniquely addressable members.
        if size > unit.size.checked_mul(4, cx).unwrap() {
            return None;
        }

        let valid_unit = match unit.kind {
            RegKind::Integer => false,
            RegKind::Float => true,
            RegKind::Vector => size.bits() == 64 || size.bits() == 128
        };

        if valid_unit {
            Some(Uniform {
                unit,
                total: size
            })
        } else {
            None
        }
    })
}

fn classify_ret<'a, Ty, C>(cx: &C, ret: &mut ArgAbi<'a, Ty>, vfp: bool)
    where Ty: TyLayoutMethods<'a, C> + Copy,
          C: LayoutOf<Ty = Ty, TyLayout = TyLayout<'a, Ty>> + HasDataLayout
{
    if !ret.layout.is_aggregate() {
        ret.extend_integer_width_to(32);
        return;
    }

    if vfp {
        if let Some(uniform) = is_homogeneous_aggregate(cx, ret) {
            ret.cast_to(uniform);
            return;
        }
    }

    let size = ret.layout.pref_pos.size;
    let bits = size.bits();
    if bits <= 32 {
        let unit = if bits <= 8 {
            Reg::i8()
        } else if bits <= 16 {
            Reg::i16()
        } else {
            Reg::i32()
        };
        ret.cast_to(Uniform {
            unit,
            total: size
        });
        return;
    }
    ret.make_indirect();
}

fn classify_arg<'a, Ty, C>(cx: &C, arg: &mut ArgAbi<'a, Ty>, vfp: bool)
    where Ty: TyLayoutMethods<'a, C> + Copy,
          C: LayoutOf<Ty = Ty, TyLayout = TyLayout<'a, Ty>> + HasDataLayout
{
    if !arg.layout.is_aggregate() {
        arg.extend_integer_width_to(32);
        return;
    }

    if vfp {
        if let Some(uniform) = is_homogeneous_aggregate(cx, arg) {
            arg.cast_to(uniform);
            return;
        }
    }

    let align = arg.layout.pref_pos.align.abi.bytes();
    let total = arg.layout.pref_pos.size;
    arg.cast_to(Uniform {
        unit: if align <= 4 { Reg::i32() } else { Reg::i64() },
        total
    });
}

pub fn compute_abi_info<'a, Ty, C>(cx: &C, fn_abi: &mut FnAbi<'a, Ty>)
    where Ty: TyLayoutMethods<'a, C> + Copy,
          C: LayoutOf<Ty = Ty, TyLayout = TyLayout<'a, Ty>> + HasDataLayout + HasTargetSpec
{
    // If this is a target with a hard-float ABI, and the function is not explicitly
    // `extern "aapcs"`, then we must use the VFP registers for homogeneous aggregates.
    let vfp = cx.target_spec().llvm_target.ends_with("hf")
        && fn_abi.conv != Conv::ArmAapcs
        && !fn_abi.c_variadic;

    if !fn_abi.ret.is_ignore() {
        classify_ret(cx, &mut fn_abi.ret, vfp);
    }

    for arg in &mut fn_abi.args {
        if arg.is_ignore() { continue; }
        classify_arg(cx, arg, vfp);
    }
}

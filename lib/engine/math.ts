// 数学の口（three を読まない）。aisns の lib/engine/math.ts の写し（キャラメイクが使う物だけ）。
// 中身は three r178 の src/math の写し（lib/engine/r178/*.mjs。aisns の tools/gen-engine-math.mjs が作った物をそのまま写した。MIT。NOTICE.md）。
// 型は写した r178 の型（./r178/*.d.mts。@types/three 0.178.1 の写し）。three は型も読まない。
import { Vector2 as Vector2R } from './r178/Vector2.mjs';
import { Vector3 as Vector3R } from './r178/Vector3.mjs';
import { Matrix3 as Matrix3R } from './r178/Matrix3.mjs';
import { Matrix4 as Matrix4R } from './r178/Matrix4.mjs';
import { Quaternion as QuaternionR } from './r178/Quaternion.mjs';
import { Euler as EulerR } from './r178/Euler.mjs';
import { Box3 as Box3R } from './r178/Box3.mjs';
import { Sphere as SphereR } from './r178/Sphere.mjs';
import { Plane as PlaneR } from './r178/Plane.mjs';
import { Ray as RayR } from './r178/Ray.mjs';
import { Spherical as SphericalR } from './r178/Spherical.mjs';
import { MathUtils as MathUtilsR } from './r178/MathUtils.mjs';

export const Vector2 = Vector2R;
export type Vector2 = Vector2R;
export const Vector3 = Vector3R;
export type Vector3 = Vector3R;
export const Matrix3 = Matrix3R;
export type Matrix3 = Matrix3R;
export const Matrix4 = Matrix4R;
export type Matrix4 = Matrix4R;
export const Quaternion = QuaternionR;
export type Quaternion = QuaternionR;
export const Euler = EulerR;
export type Euler = EulerR;
export const Box3 = Box3R;
export type Box3 = Box3R;
export const Sphere = SphereR;
export type Sphere = SphereR;
export const Plane = PlaneR;
export type Plane = PlaneR;
export const Ray = RayR;
export type Ray = RayR;
export const Spherical = SphericalR;
export type Spherical = SphericalR;
export const MathUtils = MathUtilsR;

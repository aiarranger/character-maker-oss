// 数学の口（three を読まない）。aisns の lib/engine/math.ts の写し（キャラメイクが使う物だけ）。
// 中身は three r178 の src/math の写し（lib/engine/r178/*.mjs。aisns の tools/gen-engine-math.mjs が作った物をそのまま写した。MIT。NOTICE.md）。
// 型は three の型を付ける（import type だけ。動くときに three は読まない）。
import type * as T from 'three';
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

export const Vector2 = Vector2R as unknown as typeof T.Vector2;
export type Vector2 = T.Vector2;
export const Vector3 = Vector3R as unknown as typeof T.Vector3;
export type Vector3 = T.Vector3;
export const Matrix3 = Matrix3R as unknown as typeof T.Matrix3;
export type Matrix3 = T.Matrix3;
export const Matrix4 = Matrix4R as unknown as typeof T.Matrix4;
export type Matrix4 = T.Matrix4;
export const Quaternion = QuaternionR as unknown as typeof T.Quaternion;
export type Quaternion = T.Quaternion;
export const Euler = EulerR as unknown as typeof T.Euler;
export type Euler = T.Euler;
export const Box3 = Box3R as unknown as typeof T.Box3;
export type Box3 = T.Box3;
export const Sphere = SphereR as unknown as typeof T.Sphere;
export type Sphere = T.Sphere;
export const Plane = PlaneR as unknown as typeof T.Plane;
export type Plane = T.Plane;
export const Ray = RayR as unknown as typeof T.Ray;
export type Ray = T.Ray;
export const Spherical = SphericalR as unknown as typeof T.Spherical;
export type Spherical = T.Spherical;
export const MathUtils = MathUtilsR as unknown as typeof T.MathUtils;

// 写した r178 の数学の型（*.d.mts）が読む、写していない型の代わり（three を読まない。キャラメイクは使わないので形だけ）。
// three の型（@types/three 0.178.1）の名前と、数学の型が読む欄だけを持つ。
import type { Matrix4 } from "./Matrix4.mjs";
import type { Vector3 } from "./Vector3.mjs";

export interface Camera { matrixWorld: Matrix4; matrixWorldInverse: Matrix4; projectionMatrix: Matrix4; projectionMatrixInverse: Matrix4 }
export interface BufferAttribute { count: number; itemSize: number; getX(index: number): number; getY(index: number): number; getZ(index: number): number; getW(index: number): number; setXYZ(index: number, x: number, y: number, z: number): this; setXYZW(index: number, x: number, y: number, z: number, w: number): this; setXY(index: number, x: number, y: number): this }
export type InterleavedBufferAttribute = BufferAttribute;
export interface Object3D { matrixWorld: Matrix4; updateWorldMatrix(updateParents: boolean, updateChildren: boolean): void; traverse(callback: (object: Object3D) => void): void }
export interface RGB { r: number; g: number; b: number }
export interface Cylindrical { radius: number; theta: number; y: number }
export interface Line3 { start: Vector3; end: Vector3 }
export interface Triangle { a: Vector3; b: Vector3; c: Vector3 }

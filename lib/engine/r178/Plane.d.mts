// three r178 の型（@types/three 0.178.1 の src/math/Plane.d.ts。DefinitelyTyped、MIT、lib/engine/r178/LICENSE-types）の写し（aisns の段階4 Z の単位 Z-Y から写した）。import の先だけを替えた（写していない型は ./outside.d.mts）。中身は変えない。
import { Box3 } from "./Box3.mjs";
import { Line3 } from "./outside.mjs";
import { Matrix3 } from "./Matrix3.mjs";
import { Matrix4 } from "./Matrix4.mjs";
import { Sphere } from "./Sphere.mjs";
import { Vector3 } from "./Vector3.mjs";

export class Plane {
    constructor(normal?: Vector3, constant?: number);

    /**
     * @default new THREE.Vector3( 1, 0, 0 )
     */
    normal: Vector3;

    /**
     * @default 0
     */
    constant: number;

    readonly isPlane: true;

    set(normal: Vector3, constant: number): Plane;
    setComponents(x: number, y: number, z: number, w: number): Plane;
    setFromNormalAndCoplanarPoint(normal: Vector3, point: Vector3): Plane;
    setFromCoplanarPoints(a: Vector3, b: Vector3, c: Vector3): Plane;
    clone(): this;
    copy(plane: Plane): this;
    normalize(): Plane;
    negate(): Plane;
    distanceToPoint(point: Vector3): number;
    distanceToSphere(sphere: Sphere): number;
    projectPoint(point: Vector3, target: Vector3): Vector3;
    intersectLine(line: Line3, target: Vector3): Vector3 | null;
    intersectsLine(line: Line3): boolean;
    intersectsBox(box: Box3): boolean;
    intersectsSphere(sphere: Sphere): boolean;
    coplanarPoint(target: Vector3): Vector3;
    applyMatrix4(matrix: Matrix4, optionalNormalMatrix?: Matrix3): Plane;
    translate(offset: Vector3): Plane;
    equals(plane: Plane): boolean;

    /**
     * @deprecated Use {@link Plane#intersectsLine .intersectsLine()} instead.
     */
    isIntersectionLine(l: any): any;
}

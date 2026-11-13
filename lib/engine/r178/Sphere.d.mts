// three r178 の型（@types/three 0.178.1 の src/math/Sphere.d.ts。DefinitelyTyped、MIT、lib/map-expansion/kit/types/LICENSE）の写し（段階4 Z の単位 Z-Y）。import の先だけを替えた。中身は変えない。
import { Box3 } from "./Box3.mjs";
import { Matrix4 } from "./Matrix4.mjs";
import { Plane } from "./Plane.mjs";
import { Vector3 } from "./Vector3.mjs";

export class Sphere {
    constructor(center?: Vector3, radius?: number);

    /**
     * Read-only flag to check if a given object is of type {@link Sphere}.
     */
    readonly isSphere: true;

    /**
     * @default new Vector3()
     */
    center: Vector3;

    /**
     * @default 1
     */
    radius: number;

    set(center: Vector3, radius: number): Sphere;
    setFromPoints(points: Vector3[], optionalCenter?: Vector3): Sphere;
    clone(): this;
    copy(sphere: Sphere): this;
    expandByPoint(point: Vector3): this;
    isEmpty(): boolean;
    makeEmpty(): this;
    containsPoint(point: Vector3): boolean;
    distanceToPoint(point: Vector3): number;
    intersectsSphere(sphere: Sphere): boolean;
    intersectsBox(box: Box3): boolean;
    intersectsPlane(plane: Plane): boolean;
    clampPoint(point: Vector3, target: Vector3): Vector3;
    getBoundingBox(target: Box3): Box3;
    applyMatrix4(matrix: Matrix4): Sphere;
    translate(offset: Vector3): Sphere;
    equals(sphere: Sphere): boolean;
    union(sphere: Sphere): this;

    /**
     * @deprecated Use {@link Sphere#isEmpty .isEmpty()} instead.
     */
    empty(): any;

    toJSON(): SphereJSON;
    fromJSON(json: SphereJSON): this;
}

export interface SphereJSON {
    radius: number;
    center: number[];
}

// three r178 の型（@types/three 0.178.1 の src/math/Spherical.d.ts。DefinitelyTyped、MIT、lib/map-expansion/kit/types/LICENSE）の写し（段階4 Z の単位 Z-Y）。import の先だけを替えた。中身は変えない。
import { Vector3 } from "./Vector3.mjs";

export class Spherical {
    constructor(radius?: number, phi?: number, theta?: number);

    /**
     * @default 1
     */
    radius: number;

    /**
     * @default 0
     */
    phi: number;

    /**
     * @default 0
     */
    theta: number;

    set(radius: number, phi: number, theta: number): this;
    clone(): this;
    copy(other: Spherical): this;
    makeSafe(): this;
    setFromVector3(v: Vector3): this;
    setFromCartesianCoords(x: number, y: number, z: number): this;
}

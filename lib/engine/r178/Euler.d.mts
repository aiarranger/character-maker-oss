// three r178 の型（@types/three 0.178.1 の src/math/Euler.d.ts。DefinitelyTyped、MIT、lib/engine/r178/LICENSE-types）の写し（aisns の段階4 Z の単位 Z-Y から写した）。import の先だけを替えた（写していない型は ./outside.d.mts）。中身は変えない。
import { Matrix4 } from "./Matrix4.mjs";
import { Quaternion } from "./Quaternion.mjs";
import { Vector3 } from "./Vector3.mjs";

export type EulerOrder = "XYZ" | "YXZ" | "ZXY" | "ZYX" | "YZX" | "XZY";

export type EulerTuple = [x: number, y: number, z: number, order?: EulerOrder];

export class Euler {
    constructor(x?: number, y?: number, z?: number, order?: EulerOrder);

    /**
     * @default 0
     */
    x: number;

    /**
     * @default 0
     */
    y: number;

    /**
     * @default 0
     */
    z: number;

    /**
     * @default THREE.Euler.DEFAULT_ORDER
     */
    order: EulerOrder;
    readonly isEuler: true;

    _onChangeCallback: () => void;

    set(x: number, y: number, z: number, order?: EulerOrder): Euler;
    clone(): this;
    copy(euler: Euler): this;
    setFromRotationMatrix(m: Matrix4, order?: EulerOrder, update?: boolean): Euler;
    setFromQuaternion(q: Quaternion, order?: EulerOrder, update?: boolean): Euler;
    setFromVector3(v: Vector3, order?: EulerOrder): Euler;
    reorder(newOrder: EulerOrder): Euler;
    equals(euler: Euler): boolean;
    fromArray(array: EulerTuple): Euler;
    toArray(array?: Partial<EulerTuple>, offset?: number): EulerTuple;
    _onChange(callback: () => void): this;

    static DEFAULT_ORDER: "XYZ";

    [Symbol.iterator](): Generator<string | number, void>;
}

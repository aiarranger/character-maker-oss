// 節点の木（段階4 Z「three を外す」。docs/plan/2026-11-12-z-three-removal-plan.md の 2.3 の表「Object3D の木 → lib/engine/node.ts」）。
// three r178 の src/core/Object3D.js の要るメソッドだけの写し。式と順番は r178 のまま（tools/engine-node.test.mjs で three と Object.is で突き合わせる）。
// 数学は lib/engine/math.ts だけを読む（three 全体・React は読まない）。
// 写していない物: 出来事（added・removed）・layers・toJSON・clone/copy・attach・材質や影の値。要るときに r178 から足す。
import { Vector3, Euler, Quaternion, Matrix4 } from './math';

const _v1 = new Vector3();
const _q1 = new Quaternion();
const _m1 = new Matrix4();
const _target = new Vector3();

const _position = new Vector3();
const _scale = new Vector3();
const _quaternion = new Quaternion();

const _xAxis = new Vector3(1, 0, 0);
const _yAxis = new Vector3(0, 1, 0);
const _zAxis = new Vector3(0, 0, 1);

/** 毎コマの計算が書く節点の形（three の Object3D も、この Node も満たす） */
export interface NodeLike {
  name: string;
  readonly position: Vector3;
  readonly rotation: Euler;
  readonly quaternion: Quaternion;
  readonly scale: Vector3;
  matrix: Matrix4;
  matrixWorld: Matrix4;
  matrixAutoUpdate: boolean;
  matrixWorldAutoUpdate: boolean;
  matrixWorldNeedsUpdate: boolean;
  visible: boolean;
  parent: NodeLike | null;
  children: NodeLike[];
  userData: Record<string, unknown>;
  updateMatrix(): void;
  updateMatrixWorld(force?: boolean): void;
  updateWorldMatrix(updateParents: boolean, updateChildren: boolean): void;
  getWorldPosition(target: Vector3): Vector3;
}

let _nodeId = 0;

/** three r178 の Object3D の写し（要るメソッドだけ） */
export class Node implements NodeLike {
  static DEFAULT_UP = new Vector3(0, 1, 0);
  static DEFAULT_MATRIX_AUTO_UPDATE = true;
  static DEFAULT_MATRIX_WORLD_AUTO_UPDATE = true;

  readonly isNode = true;
  /** three の Object3D と同じ印（lookAt・getObjectByProperty などが見る） */
  readonly isObject3D = true;
  readonly id = _nodeId++;
  name = '';
  type = 'Object3D';
  parent: Node | null = null;
  children: Node[] = [];
  up = Node.DEFAULT_UP.clone();

  readonly position = new Vector3();
  readonly rotation = new Euler();
  readonly quaternion = new Quaternion();
  readonly scale = new Vector3(1, 1, 1);

  matrix = new Matrix4();
  matrixWorld = new Matrix4();
  matrixAutoUpdate = Node.DEFAULT_MATRIX_AUTO_UPDATE;
  matrixWorldAutoUpdate = Node.DEFAULT_MATRIX_WORLD_AUTO_UPDATE;
  matrixWorldNeedsUpdate = false;

  visible = true;
  frustumCulled = true;
  renderOrder = 0;
  userData: Record<string, unknown> = {};

  /** lookAt の向きを分ける印（Camera・Light の写しが true にする） */
  isCamera?: boolean;
  isLight?: boolean;

  constructor() {
    const rotation = this.rotation;
    const quaternion = this.quaternion;
    // r178 と同じく、rotation と quaternion は片方を書くともう片方を書き直す
    rotation._onChange(() => { quaternion.setFromEuler(rotation, false); });
    quaternion._onChange(() => { rotation.setFromQuaternion(quaternion, undefined, false); });
  }

  applyMatrix4(matrix: Matrix4) {
    if (this.matrixAutoUpdate) this.updateMatrix();
    this.matrix.premultiply(matrix);
    this.matrix.decompose(this.position, this.quaternion, this.scale);
  }

  applyQuaternion(q: Quaternion) {
    this.quaternion.premultiply(q);
    return this;
  }

  setRotationFromAxisAngle(axis: Vector3, angle: number) { this.quaternion.setFromAxisAngle(axis, angle); }
  setRotationFromEuler(euler: Euler) { this.quaternion.setFromEuler(euler, true); }
  setRotationFromMatrix(m: Matrix4) { this.quaternion.setFromRotationMatrix(m); }
  setRotationFromQuaternion(q: Quaternion) { this.quaternion.copy(q); }

  rotateOnAxis(axis: Vector3, angle: number) {
    _q1.setFromAxisAngle(axis, angle);
    this.quaternion.multiply(_q1);
    return this;
  }

  rotateOnWorldAxis(axis: Vector3, angle: number) {
    _q1.setFromAxisAngle(axis, angle);
    this.quaternion.premultiply(_q1);
    return this;
  }

  rotateX(angle: number) { return this.rotateOnAxis(_xAxis, angle); }
  rotateY(angle: number) { return this.rotateOnAxis(_yAxis, angle); }
  rotateZ(angle: number) { return this.rotateOnAxis(_zAxis, angle); }

  translateOnAxis(axis: Vector3, distance: number) {
    _v1.copy(axis).applyQuaternion(this.quaternion);
    this.position.add(_v1.multiplyScalar(distance));
    return this;
  }

  translateX(distance: number) { return this.translateOnAxis(_xAxis, distance); }
  translateY(distance: number) { return this.translateOnAxis(_yAxis, distance); }
  translateZ(distance: number) { return this.translateOnAxis(_zAxis, distance); }

  localToWorld(vector: Vector3) {
    this.updateWorldMatrix(true, false);
    return vector.applyMatrix4(this.matrixWorld);
  }

  worldToLocal(vector: Vector3) {
    this.updateWorldMatrix(true, false);
    return vector.applyMatrix4(_m1.copy(this.matrixWorld).invert());
  }

  lookAt(x: Vector3 | number, y?: number, z?: number) {
    // This method does not support objects having non-uniformly-scaled parent(s)
    if (typeof x === 'object' && x.isVector3) {
      _target.copy(x);
    } else {
      _target.set(x as number, y as number, z as number);
    }
    const parent = this.parent;
    this.updateWorldMatrix(true, false);
    _position.setFromMatrixPosition(this.matrixWorld);
    if (this.isCamera || this.isLight) {
      _m1.lookAt(_position, _target, this.up);
    } else {
      _m1.lookAt(_target, _position, this.up);
    }
    this.quaternion.setFromRotationMatrix(_m1);
    if (parent) {
      _m1.extractRotation(parent.matrixWorld);
      _q1.setFromRotationMatrix(_m1);
      this.quaternion.premultiply(_q1.invert());
    }
  }

  add(...objects: Node[]): this {
    if (objects.length > 1) {
      for (let i = 0; i < objects.length; i++) this.add(objects[i]);
      return this;
    }
    const object = objects[0];
    if (object === this) {
      console.error("Node.add: object can't be added as a child of itself.", object);
      return this;
    }
    if (object && object.isNode) {
      object.removeFromParent();
      object.parent = this;
      this.children.push(object);
    } else {
      console.error('Node.add: object not an instance of Node.', object);
    }
    return this;
  }

  remove(...objects: Node[]): this {
    if (objects.length > 1) {
      for (let i = 0; i < objects.length; i++) this.remove(objects[i]);
      return this;
    }
    const object = objects[0];
    const index = this.children.indexOf(object);
    if (index !== -1) {
      object.parent = null;
      this.children.splice(index, 1);
    }
    return this;
  }

  removeFromParent() {
    const parent = this.parent;
    if (parent !== null) parent.remove(this);
    return this;
  }

  clear() { return this.remove(...this.children); }

  getObjectById(id: number) { return this.getObjectByProperty('id', id); }
  getObjectByName(name: string) { return this.getObjectByProperty('name', name); }

  getObjectByProperty(name: string, value: unknown): Node | undefined {
    if ((this as unknown as Record<string, unknown>)[name] === value) return this;
    for (let i = 0, l = this.children.length; i < l; i++) {
      const object = this.children[i].getObjectByProperty(name, value);
      if (object !== undefined) return object;
    }
    return undefined;
  }

  getWorldPosition(target: Vector3) {
    this.updateWorldMatrix(true, false);
    return target.setFromMatrixPosition(this.matrixWorld);
  }

  getWorldQuaternion(target: Quaternion) {
    this.updateWorldMatrix(true, false);
    this.matrixWorld.decompose(_position, target, _scale);
    return target;
  }

  getWorldScale(target: Vector3) {
    this.updateWorldMatrix(true, false);
    this.matrixWorld.decompose(_position, _quaternion, target);
    return target;
  }

  getWorldDirection(target: Vector3) {
    this.updateWorldMatrix(true, false);
    const e = this.matrixWorld.elements;
    return target.set(e[8], e[9], e[10]).normalize();
  }

  traverse(callback: (node: Node) => void) {
    callback(this);
    const children = this.children;
    for (let i = 0, l = children.length; i < l; i++) children[i].traverse(callback);
  }

  traverseVisible(callback: (node: Node) => void) {
    if (this.visible === false) return;
    callback(this);
    const children = this.children;
    for (let i = 0, l = children.length; i < l; i++) children[i].traverseVisible(callback);
  }

  traverseAncestors(callback: (node: Node) => void) {
    const parent = this.parent;
    if (parent !== null) {
      callback(parent);
      parent.traverseAncestors(callback);
    }
  }

  updateMatrix() {
    this.matrix.compose(this.position, this.quaternion, this.scale);
    this.matrixWorldNeedsUpdate = true;
  }

  updateMatrixWorld(force?: boolean) {
    if (this.matrixAutoUpdate) this.updateMatrix();
    if (this.matrixWorldNeedsUpdate || force) {
      if (this.matrixWorldAutoUpdate === true) {
        if (this.parent === null) {
          this.matrixWorld.copy(this.matrix);
        } else {
          this.matrixWorld.multiplyMatrices(this.parent.matrixWorld, this.matrix);
        }
      }
      this.matrixWorldNeedsUpdate = false;
      force = true;
    }
    // make sure descendants are updated if required
    const children = this.children;
    for (let i = 0, l = children.length; i < l; i++) children[i].updateMatrixWorld(force);
  }

  updateWorldMatrix(updateParents: boolean, updateChildren: boolean) {
    const parent = this.parent;
    if (updateParents === true && parent !== null) parent.updateWorldMatrix(true, false);
    if (this.matrixAutoUpdate) this.updateMatrix();
    if (this.matrixWorldAutoUpdate === true) {
      if (this.parent === null) {
        this.matrixWorld.copy(this.matrix);
      } else {
        this.matrixWorld.multiplyMatrices(this.parent.matrixWorld, this.matrix);
      }
    }
    // make sure descendants are updated
    if (updateChildren === true) {
      const children = this.children;
      for (let i = 0, l = children.length; i < l; i++) children[i].updateWorldMatrix(false, true);
    }
  }
}

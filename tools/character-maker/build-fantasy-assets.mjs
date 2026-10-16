#!/usr/bin/env node
/** Deterministic, offline CC0 beta-VRoid conversion. Original VRMs and comic assets are immutable. */
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { Document, NodeIO, Accessor } from '@gltf-transform/core';
import { KHRMaterialsUnlit } from '@gltf-transform/extensions';
import { copyToDocument, prune, dedup, sparse, join } from '@gltf-transform/functions';
import { Matrix4, Vector3 } from 'three';
import sharp from 'sharp';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const HOME = path.join(ROOT, 'parts');
const SOURCES = path.join(HOME, 'sources/fantasy-v1');
const OUT = path.join(HOME, 'assets/fantasy-v1');
const io = new NodeIO().registerExtensions([KHRMaterialsUnlit]);
const sha = value => createHash('sha256').update(value).digest('hex');
const args = process.argv.slice(2);
const argument = key => args.find(a => a.startsWith(`${key}=`))?.slice(key.length + 1);
const SOURCE_DEFS = {
  victoria: {
    logicalId: 'vroid-beta-avatar-sample-4-victoria-rubin', file: 'Victoria_Rubin.vrm', originalFile: 'AvatarSample_G.vrm',
    probe: 'avatarsample_g', sha256: 'ad5750ce944b708155abb7ac6807e19d7f5e73a939f0ee32daf0539acfca57c0',
    download: 'https://opengameart.org/sites/default/files/avatarsample_g.zip',
    licenseArticle: 'https://vroid.pixiv.help/hc/ja/articles/360014900233',
    officialModel: 'https://hub.vroid.com/characters/2792872861023597723/models/5013769147837660446',
  },
  sakurada: {
    logicalId: 'vroid-beta-sakurada-fumiriya-normal', file: 'Sakurada_Fumiriya.vrm', originalFile: 'Sakurada Fumiriya.vrm',
    probe: 'sakurada_fumiriya', sha256: 'a36e91b81518c59f6da0e3f34a176b79090a8c68cc6bd5fe03c1560744b283f3',
    download: 'https://opengameart.org/sites/default/files/sakurada_fumiriya.zip',
    licenseArticle: 'https://vroid.pixiv.help/hc/en-us/articles/360014788554-Sakurada-Fumiriya',
    officialModel: 'https://hub.vroid.com/en/characters/6912965120285194650/models/2261505926203110716',
  },
  hairSampleMale: {
    logicalId: 'vroid-beta-hairsample-male', file: 'HairSample_Male.vrm', originalFile: 'HairSample_Male.vrm',
    probe: 'hairsample_male', sha256: '4af2194f90ba846f3b13c00b50b14262419062d2171d17de922bcce2f89979c7',
    download: 'https://opengameart.org/sites/default/files/hairsample_male.zip',
    licenseArticle: 'https://vroid.pixiv.help/hc/ja/articles/4402614652569',
  },
};
const BASES = { feminine: 'victoria', masculine: 'sakurada' };
const HAIRS = {
  animeParted: { source: 'sakurada', label: 'エアリーショート' },
  sidePony: { source: 'victoria', label: 'サイドポニー' },
  layered: { source: 'hairSampleMale', label: 'レイヤーショート' },
};
const MORPHS = ['bodySlim', 'bodyBroad', 'faceRound', 'faceAngular', 'eyesRound', 'eyesSlender', 'mouthSmile', 'mouthFull'];
const HEAD = 'J_Bip_C_Head';
const WRAPPER = 'maker-fantasy-facing';
const TEXTURE_FINISH = {
  version: 'source-contrast-1',
  hair: { grayscale: 'Rec.709 linear luminance', contrastGamma: 1.55 },
  skin: { contrastGamma: 1.10, peak: .995 },
  whiteCloth: { contrastGamma: 1.32, peak: .97, brightRange: [.55, .82], saturationRange: [.08, .28] },
};
const gammaEOTF = v => v ** 2.2; // Matches @pixiv/three-vrm 3.5.5 VRMMaterialsV0CompatPlugin.
const srgbToLinear = v => v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
const linearToSrgb = v => v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055;
const smooth = (a, b, x) => { const t = Math.max(0, Math.min(1, (x - a) / (b - a))); return t * t * (3 - 2 * t); };
const gaussian = (x, width) => Math.exp(-((x / width) ** 2));
const compact = xs => xs.map(x => Number(x.toFixed(9)));

function parseVrm(bytes) {
  assert.equal(bytes.readUInt32LE(0), 0x46546c67);
  assert.equal(bytes.readUInt32LE(4), 2);
  assert.equal(bytes.readUInt32LE(8), bytes.length);
  const length = bytes.readUInt32LE(12);
  return { json: JSON.parse(bytes.subarray(20, 20 + length)), binary: bytes.subarray(28 + length) };
}

async function importSources(from) {
  await fs.mkdir(SOURCES, { recursive: true });
  const records = [];
  for (const [id, def] of Object.entries(SOURCE_DEFS)) {
    const source = path.join(from, def.probe, def.originalFile), dest = path.join(SOURCES, def.file);
    const bytes = await fs.readFile(source);
    assert.equal(sha(bytes), def.sha256, `Original SHA mismatch: ${id}`);
    let old; try { old = await fs.readFile(dest); } catch (e) { if (e.code !== 'ENOENT') throw e; }
    if (old) assert.deepEqual(old, bytes, 'Refuse replacing an original');
    else await fs.writeFile(dest, bytes);
    const { json } = parseVrm(bytes);
    records.push({ id, ...def, bytes: bytes.length, author: 'VRoid Project / pixiv', license: 'CC0-1.0',
      embeddedMeta: json.extensions.VRM.meta,
      licenseEvidence: id === 'hairSampleMale' ? 'Official beta sample license article and embedded licenseName=CC0.' : 'Official individual model article explicitly CC0. Embedded licenseName=Other links to the original Hub permission settings (redistribution/modification/commercial allowed); it is not the newer A-Z sample license.',
    });
  }
  await fs.writeFile(path.join(SOURCES, 'provenance.json'), JSON.stringify({ schemaVersion: 1, author: 'VRoid Project / pixiv', license: 'CC0-1.0', mirrorPage: 'https://opengameart.org/content/vroid-studio-cc0-models', officialBetaLicense: 'https://vroid.pixiv.help/hc/ja/articles/4402614652569', excludedCurrentSamples: 'https://vroid.pixiv.help/hc/ja/articles/4402394424089-AvatarSample-A-Z', note: 'Only these immutable beta-era models are inputs. Modern AvatarSample A-Z terms do not authorize character-creation service reuse. ZIPs are not duplicated.', originals: records }, null, 2) + '\n');
}

async function verifySources() {
  for (const def of Object.values(SOURCE_DEFS)) assert.equal(sha(await fs.readFile(path.join(SOURCES, def.file))), def.sha256);
}

function imageBytes(json, binary, textureIndex) {
  const image = json.images[json.textures[textureIndex].source], view = json.bufferViews[image.bufferView];
  return binary.subarray(view.byteOffset ?? 0, (view.byteOffset ?? 0) + view.byteLength);
}

async function bakedColor(json, binary, source, role) {
  const neutral = role === 'hair' || role === 'brow';
  const properties = source.textureProperties ?? {}, vectors = source.vectorProperties ?? {};
  const main = properties._MainTex;
  let decoded = main !== undefined
    ? await sharp(imageBytes(json, binary, main)).resize({ width: 1024, height: 1024, fit: 'inside', withoutEnlargement: true }).ensureAlpha().raw().toBuffer({ resolveWithObject: true })
    : { data: Buffer.from([255, 255, 255, 255]), info: { width: 1, height: 1, channels: 4 } };
  const color = (vectors._Color ?? [1, 1, 1, 1]).map((x, i) => i === 3 ? x : gammaEOTF(x));
  const emission = (vectors._EmissionColor ?? [0, 0, 0]).slice(0, 3).map(gammaEOTF);
  let emissivePixels = null;
  if (emission.some(x => x > 0) && properties._EmissionMap !== undefined) emissivePixels = await sharp(imageBytes(json, binary, properties._EmissionMap)).resize(decoded.info.width, decoded.info.height, { fit: 'fill' }).ensureAlpha().raw().toBuffer();
  // Unlit carries the source main texture + linear emission in one sRGB image.
  // Editable hair/brows use a neutral luminance rendition of that image, while
  // keeping original alpha. Colored accessories retain their source colors.
  const data = Buffer.from(decoded.data);
  const values = new Float32Array(data.length / 4 * 3);
  let neutralPeak = 0;
  for (let i = 0; i < data.length; i += 4) {
    const rgb = [0, 1, 2].map(c => srgbToLinear(data[i + c] / 255) * color[c] + (emissivePixels ? srgbToLinear(emissivePixels[i + c] / 255) * emission[c] : emission[c]));
    const light = role === 'hair' ? .2126 * rgb[0] + .7152 * rgb[1] + .0722 * rgb[2] : Math.max(...rgb);
    neutralPeak = Math.max(neutralPeak, data[i + 3] > 8 ? light : 0);
    for (let c = 0; c < 3; c++) values[i / 4 * 3 + c] = neutral ? light : rgb[c];
    data[i + 3] = Math.round(data[i + 3] * color[3]);
  }
  for (let i = 0; i < data.length; i += 4) {
    const rgb = [0, 1, 2].map(c => Math.min(1, Math.max(0, linearToSrgb(values[i / 4 * 3 + c] / (neutral && neutralPeak > 0 ? neutralPeak : 1)))));
    if (role === 'hair') {
      // Retain the authored strands. Expanding their existing midtone range
      // avoids the flat gray produced by max-channel monochrome conversion.
      rgb.fill(rgb[0] ** TEXTURE_FINISH.hair.contrastGamma);
    } else if (role === 'skin' || role === 'cloth') {
      const luminance = .2126 * rgb[0] + .7152 * rgb[1] + .0722 * rgb[2];
      const settings = role === 'skin' ? TEXTURE_FINISH.skin : TEXTURE_FINISH.whiteCloth;
      // Only the pale, low-saturation fabric is treated; dark uniforms, gold,
      // pink trim, irises and painted lip/eyelash materials keep their colors.
      const amount = role === 'skin' ? 1 : smooth(...settings.brightRange, luminance) * (1 - smooth(...settings.saturationRange, Math.max(...rgb) - Math.min(...rgb)));
      const target = settings.peak * luminance ** settings.contrastGamma;
      const ratio = luminance > 0 ? (luminance + (target - luminance) * amount) / luminance : 1;
      for (let c = 0; c < 3; c++) rgb[c] *= ratio;
    }
    for (let c = 0; c < 3; c++) data[i + c] = Math.round(Math.min(1, Math.max(0, rgb[c])) * 255);
  }
  return sharp(data, { raw: decoded.info }).png({ compressionLevel: 9 }).toBuffer();
}

async function loadSource(id) {
  const def = SOURCE_DEFS[id], bytes = await fs.readFile(path.join(SOURCES, def.file));
  const { json, binary } = parseVrm(bytes);
  const original = structuredClone(json), vrm = json.extensions.VRM;
  const resources = { '@glb.bin': binary }, images = [], textures = [];
  const materialRecords = [];
  for (let i = 0; i < json.materials.length; i++) {
    const material = json.materials[i], source = vrm.materialProperties[i], name = material.name;
    const accessory = id === 'victoria' && name === 'F00_000_Hair_00_HAIR_03';
    const brow = /FaceBrow/.test(name), hair = /_HAIR/.test(name) && !accessory;
    const uri = `baked-material-${i}.png`;
    const finishRole = hair ? 'hair' : brow ? 'brow' : /_SKIN$/.test(name) ? 'skin' : /_CLOTH$/.test(name) ? 'cloth' : 'other';
    resources[uri] = await bakedColor(original, binary, source, finishRole);
    images.push({ uri }); textures.push({ source: images.length - 1 });
    const transparent = !!source.keywordMap?._ALPHABLEND_ON;
    const cutoff = !!source.keywordMap?._ALPHATEST_ON;
    materialRecords.push({ originalName: name, sourceShader: source.shader, sourceParameters: source, conversion: 'VRM0 gammaEOTF 2.2 factors; main texture and emission combined in linear color; original alpha/cull preserved; KHR_materials_unlit', neutralColor: hair || brow, textureFinishRole: finishRole });
    json.materials[i] = { name: brow ? 'maker-brow' : hair ? 'maker-hair' : accessory ? 'maker-hair-accessory' : name,
      pbrMetallicRoughness: { baseColorTexture: { index: textures.length - 1 }, baseColorFactor: [1, 1, 1, 1], metallicFactor: 0, roughnessFactor: 1 },
      doubleSided: (source.floatProperties?._CullMode ?? 2) === 0,
      alphaMode: transparent ? 'BLEND' : cutoff ? 'MASK' : 'OPAQUE',
      ...(cutoff ? { alphaCutoff: source.floatProperties?._Cutoff ?? 0.5 } : {}),
      extensions: { KHR_materials_unlit: {} }, extras: { originalName: name, sourceShader: source.shader, sourceColor: source.vectorProperties?._Color, sourceEmission: source.vectorProperties?._EmissionColor },
    };
  }
  json.images = images; json.textures = textures; delete json.samplers;
  delete json.extensions; json.extensionsUsed = ['KHR_materials_unlit']; delete json.extensionsRequired;
  const removedVertexColors = [];
  for (const mesh of json.meshes) {
    const targetNames = mesh.primitives[0].extras?.targetNames ?? [];
    if (targetNames.length) mesh.extras = { ...mesh.extras, targetNames };
    for (const p of mesh.primitives) if (p.attributes.COLOR_0 !== undefined) {
      removedVertexColors.push({ mesh: mesh.name, accessor: p.attributes.COLOR_0 }); delete p.attributes.COLOR_0;
    }
  }
  const doc = await io.readJSON({ json, resources });
  const head = doc.getRoot().listNodes().find(n => n.getName() === HEAD);
  return { doc, vrm, original, materialRecords, removedVertexColors, head };
}

function sourceSkinSignatures(doc) {
  return doc.getRoot().listSkins().map(s => ({ name: s.getName(), joints: s.listJoints().map(n => ({ name: n.getName(), translation: n.getTranslation(), rotation: n.getRotation(), scale: n.getScale() })), inverseBind: Array.from(s.getInverseBindMatrices().getArray()) }));
}
function profileOf(source) {
  const nodes = source.doc.getRoot().listNodes();
  const world = name => new Vector3().setFromMatrixPosition(new Matrix4().fromArray(nodes.find(n => n.getName() === name).getWorldMatrix()));
  const head = world(HEAD), neck = world('J_Bip_C_Neck');
  const face = nodes.find(n => n.getName() === 'Face').getMesh();
  function center(match) {
    const prim = face.listPrimitives().find(p => p.getMaterial().getExtras().originalName.includes(match));
    const xs = [], ys = [], zs = [], indices = new Set(prim.getIndices().getArray()), pos = prim.getAttribute('POSITION');
    for (const i of indices) { const [x,y,z] = pos.getElement(i, []); xs.push(x); ys.push(y); zs.push(z); }
    return [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2, (Math.min(...zs) + Math.max(...zs)) / 2];
  }
  const iris = center('EyeIris'), mouth = center('FaceMouth');
  const eyeX = Math.abs(world('J_Adj_L_FaceEye').x) * 1.85;
  const all = face.listPrimitives()[0].getAttribute('POSITION').getArray();
  let chin = Infinity; for (let i = 1; i < all.length; i += 3) chin = Math.min(chin, all[i]);
  return { head: head.toArray(), neck: neck.toArray(), eyeY: iris[1], eyeX, mouthY: mouth[1] - .003, chinY: chin,
    shoulderY: world('J_Bip_L_UpperArm').y, hipY: world('J_Bip_C_Hips').y, height: scalpBounds(source).max[1] + head.y,
  };
}

function morphology(name, [x,y,z], p, role) {
  if (name.startsWith('body')) {
    if (role !== 'Body') return [0,0,0];
    const direction = name === 'bodySlim' ? -1 : 1;
    const lowerNeck = 1 - smooth(p.neck[1] - .07, p.neck[1] - .015, y);
    const torso = smooth(p.hipY - .24, p.hipY - .02, y) * (1 - smooth(.18, .29, Math.abs(x))) * lowerNeck;
    const arm = smooth(.16, .28, Math.abs(x)) * (1 - smooth(.56, .72, Math.abs(x))) * gaussian(y - p.shoulderY, .10);
    const leg = (1 - smooth(p.hipY - .26, p.hipY - .08, y)) * smooth(.10, .28, y);
    const legCenter = Math.sign(x) * .075;
    return [direction * (.18 * x * torso + .13 * (x - legCenter) * leg), direction * .13 * (y - p.shoulderY) * arm, direction * (.18 * torso + .13 * arm + .12 * leg) * (z + .003)];
  }
  if (role !== 'Face') return [0,0,0];
  const front = 1 - smooth(-.06, -.015, z);
  if (name.startsWith('face')) {
    const lower = gaussian(y - (p.chinY + .032), .041) * (1 - smooth(p.eyeY - .060, p.eyeY - .004, y)) * front;
    const tip = gaussian(y - p.chinY, .025) * front;
    return name === 'faceRound' ? [x * .12 * lower, .006 * tip, -.002 * lower] : [-x * .12 * lower, -.007 * tip, .002 * lower];
  }
  if (name.startsWith('eyes')) {
    const cx = x < 0 ? -p.eyeX : p.eyeX;
    const socket = gaussian(x - cx, .044) * gaussian(y - p.eyeY, .042) * (1 - smooth(p.eyeY + .010, p.eyeY + .025, y)) * front;
    const vertical = name === 'eyesRound' ? .30 : -.29;
    // Keep the upper lid/forehead anchored underneath the unchanged fringe.
    // The lower socket, lashes, sclera and iris share the same continuous field.
    return [(x - cx) * (name === 'eyesRound' ? .035 : .11) * socket, Math.min(0, y - p.eyeY - .020) * vertical * socket, 0];
  }
  if (name.startsWith('mouth')) {
    const lip = gaussian(x, .035) * gaussian(y - p.mouthY, .020) * front;
    return name === 'mouthSmile' ? [x * .22 * lip, .005 * Math.min(1, Math.abs(x) / .025) * lip, 0]
      : [x * .075 * lip, (y - p.mouthY) * .50 * lip, -.003 * lip];
  }
  return [0,0,0];
}

function addMorphs(source) {
  const doc = source.doc, profile = profileOf(source), buffer = doc.getRoot().listBuffers()[0];
  const report = {};
  for (const node of doc.getRoot().listNodes()) {
    const mesh = node.getMesh(), role = node.getName(); if (!mesh || !['Face','Body'].includes(role)) continue;
    const names = role === 'Face' ? MORPHS.slice(2) : MORPHS.slice(0, 2);
    const originalNames = mesh.getExtras().targetNames ?? [];
    // Target arrays shared by the original split primitives stay index-identical.
    const cache = new Map();
    for (const prim of mesh.listPrimitives()) {
      const pos = prim.getAttribute('POSITION');
      if (!cache.has(pos)) {
        const changes = new Map();
        for (const name of names) {
          const data = new Float32Array(pos.getCount() * 3);
          let maximum = 0;
          for (let i = 0; i < pos.getCount(); i++) { const delta = morphology(name, pos.getElement(i, []), profile, role); data.set(delta,i*3); maximum = Math.max(maximum,Math.hypot(...delta)); }
          changes.set(name, doc.createAccessor(`${role}-${name}`).setType(Accessor.Type.VEC3).setArray(data).setBuffer(buffer));
          report[name] = { maximumDeltaMeters: maximum };
        }
        cache.set(pos, changes);
      }
      for (const name of names) prim.addTarget(doc.createPrimitiveTarget(name).setAttribute('POSITION',cache.get(pos).get(name)));
    }
    mesh.setExtras({ ...mesh.getExtras(), originalTargetCount: originalNames.length, targetNames: [...originalNames,...names] });
    mesh.setWeights([...(mesh.getWeights().length ? mesh.getWeights() : originalNames.map(()=>0)),...names.map(()=>0)]);
  }
  return { profile, report };
}

function scalpBounds(source) {
  const bodyNode = source.doc.getRoot().listNodes().find(n=>n.getName()==='Body');
  const prim = bodyNode.getMesh().listPrimitives()[3];
  assert(prim.getMaterial().getExtras().originalName.endsWith('_SKIN'));
  const transform = new Matrix4().fromArray(source.head.getWorldMatrix()).invert().multiply(new Matrix4().fromArray(bodyNode.getWorldMatrix()));
  const min = [Infinity,Infinity,Infinity], max = [-Infinity,-Infinity,-Infinity];
  for (const i of new Set(prim.getIndices().getArray())) {
    const p = new Vector3(...prim.getAttribute('POSITION').getElement(i,[])).applyMatrix4(transform).toArray();
    for (let j=0;j<3;j++){min[j]=Math.min(min[j],p[j]);max[j]=Math.max(max[j],p[j]);}
  }
  return { min, max };
}

function usedBounds(doc) {
  const min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];
  for(const n of doc.getRoot().listNodes())if(n.getMesh())for(const p of n.getMesh().listPrimitives()){
    const matrix=new Matrix4().fromArray(n.getWorldMatrix());
    for(const i of new Set(p.getIndices().getArray())){const v=new Vector3(...p.getAttribute('POSITION').getElement(i,[])).applyMatrix4(matrix).toArray();for(let c=0;c<3;c++){min[c]=Math.min(min[c],v[c]);max[c]=Math.max(max[c],v[c]);}}
  }
  return {min:compact(min),max:compact(max),height:max[1]-min[1]};
}

async function finish(doc, file, info, expectedSkins=null) {
  const unlit = doc.createExtension(KHRMaterialsUnlit);
  for (const material of doc.getRoot().listMaterials()) material.setExtension('KHR_materials_unlit', unlit.createUnlit());
  const buffer=doc.getRoot().listBuffers()[0]??doc.createBuffer('canonical');
  for(const a of doc.getRoot().listAccessors())a.setBuffer(buffer);
  // Preserve empty source bone subtrees: they are part of the original rig contract.
  await doc.transform(prune({keepAttributes:true,keepExtras:true,keepLeaves:true}),dedup({propertyTypes:['Accessor','Texture','Material']}),sparse());
  if (info.kind === 'hair') await doc.transform(join({ keepNamed: false }));
  let encoded=Buffer.from(await io.writeBinary(doc));
  const size=encoded.readUInt32LE(12),json=JSON.parse(encoded.subarray(20,20+size));
  const nodes=new Map(doc.getRoot().listNodes().map(n=>[n.getName(),n]));
  for(const n of json.nodes??[]){const source=nodes.get(n.name);if(source){delete n.matrix;Object.assign(n,{translation:source.getTranslation(),rotation:source.getRotation(),scale:source.getScale()});}}
  const raw=Buffer.from(JSON.stringify(json)),padded=Buffer.alloc(Math.ceil(raw.length/4)*4,0x20);raw.copy(padded);
  const tail=encoded.subarray(20+size),header=Buffer.from(encoded.subarray(0,20));header.writeUInt32LE(20+padded.length+tail.length,8);header.writeUInt32LE(padded.length,12);encoded=Buffer.concat([header,padded,tail]);
  const read=await io.readBinary(encoded);
  for (const material of read.getRoot().listMaterials()) assert(material.getExtension('KHR_materials_unlit'), 'Unlit extension lost');
  if(expectedSkins){const actual=sourceSkinSignatures(read);assert.equal(actual.length,expectedSkins.length);for(let s=0;s<actual.length;s++)assert.deepEqual(actual[s],expectedSkins[s],'Native rest/IBM changed');}
  for(const texture of read.getRoot().listTextures()){const m=await sharp(texture.getImage()).metadata();assert(m.width<=1024&&m.height<=1024);}
  assert(encoded.length<25*1024*1024);
  await fs.writeFile(path.join(OUT,file),encoded);
  const entry={file,bytes:encoded.length,sha256:sha(encoded),...info,bounds:usedBounds(read)};
  console.log(JSON.stringify({file,bytes:encoded.length,height:entry.bounds.height}));
  return entry;
}

async function buildBody(base) {
  const source=await loadSource(BASES[base]),doc=source.doc,skins=sourceSkinSignatures(doc);
  const scalp=scalpBounds(source),morphs=addMorphs(source);
  const primitiveReport=[];
  for(const node of doc.getRoot().listNodes())if(node.getMesh()){
    const mesh=node.getMesh();
    for(const [index,prim]of [...mesh.listPrimitives()].entries())if(/_HAIR/.test(prim.getMaterial().getExtras().originalName)){
      primitiveReport.push({mesh:node.getName(),primitive:index,originalMaterial:prim.getMaterial().getExtras().originalName});mesh.removePrimitive(prim);
    }
    if(!mesh.listPrimitives().length)node.setMesh(null);
  }
  const scene=doc.getRoot().listScenes()[0],children=[...scene.listChildren()],wrapper=doc.createNode(WRAPPER).setRotation([0,1,0,0]);
  for(const child of children){scene.removeChild(child);wrapper.addChild(child);}scene.addChild(wrapper);
  const humanBones=Object.fromEntries(source.vrm.humanoid.humanBones.map(b=>[b.bone,source.original.nodes[b.node].name]));
  const characterMaker={schemaVersion:1,style:'fantasy',base,front:'+Z',sourceFront:'-Z',up:'+Y',unit:'meter',head:HEAD,hairAnchor:HEAD,rightHand:humanBones.rightHand,humanBones,wrapper:WRAPPER,sourceLogicalId:SOURCE_DEFS[BASES[base]].logicalId};
  doc.getRoot().setExtras({characterMaker,sourceVrmExpressions:source.vrm.blendShapeMaster,sourceMeshNames:source.original.meshes.map(m=>m.name)});
  return finish(doc,`${base}.glb`,{kind:'body',base,source:BASES[base],contract:characterMaker,scalpHeadLocal:scalp,morphs,
    originalExpressionTargetCount:source.original.meshes[0].primitives[0].targets.length,
    removedHairPrimitives:primitiveReport,removedUnusedVertexColors:source.removedVertexColors,
    skeleton:{skinCount:skins.length,jointCounts:skins.map(s=>s.joints.length),head:HEAD,root:WRAPPER,headWorldMatrix:compact(source.head.getWorldMatrix()),signatureSha256:sha(JSON.stringify(skins))},
    materialConversion:source.materialRecords,
  },skins);
}

async function buildHair(id,base) {
  const donor=await loadSource(HAIRS[id].source),recipient=await loadSource(BASES[base]);
  const from=scalpBounds(donor),to=scalpBounds(recipient);
  const scale=from.min.map((_,i)=>(to.max[i]-to.min[i])/(from.max[i]-from.min[i]));
  const offset=from.min.map((_,i)=>to.min[i]-from.min[i]*scale[i]);
  const identical=HAIRS[id].source===BASES[base];
  const doc=new Document(),scene=doc.createScene('Hair'),mesh=doc.createMesh(`hair-${id}`),node=doc.createNode(`hair-${id}`).setMesh(mesh);scene.addChild(node);doc.createBuffer('hair');
  const headInverse=new Matrix4().fromArray(donor.head.getWorldMatrix()).invert();
  const records=[];let maximumDelta=0,maximumVertex=null;
  for(const srcNode of donor.doc.getRoot().listNodes())if(srcNode.getMesh()){
    const skin=srcNode.getSkin(),jointWorld=skin.listJoints().map(n=>new Matrix4().fromArray(n.getWorldMatrix())),ibm=skin.getInverseBindMatrices();
    const sourcePrimitives=srcNode.getMesh().listPrimitives();
    for(let pi=0;pi<sourcePrimitives.length;pi++){
      const src=sourcePrimitives[pi];if(!/_HAIR/.test(src.getMaterial().getExtras().originalName))continue;
      const copied=copyToDocument(doc,donor.doc,[src]).get(src);
      const pos=src.getAttribute('POSITION'),joints=src.getAttribute('JOINTS_0'),weights=src.getAttribute('WEIGHTS_0');
      // Pack only the indices belonging to this original primitive. No triangle
      // is removed; this avoids retaining whole-body data in the HairBack package.
      const map=new Map(),indices=[];for(const i of src.getIndices().getArray()){if(!map.has(i))map.set(i,map.size);indices.push(map.get(i));}
      const data=new Float32Array(map.size*3),uv=src.getAttribute('TEXCOORD_0'),uvData=new Float32Array(map.size*2);
      for(const [i,at]of map){const point=new Vector3(...pos.getElement(i,[])),js=joints.getElement(i,[]),ws=weights.getElement(i,[]),baked=new Vector3();
        for(let k=0;k<4;k++)if(ws[k])baked.addScaledVector(point.clone().applyMatrix4(new Matrix4().fromArray(ibm.getElement(js[k],[]))).applyMatrix4(jointWorld[js[k]]),ws[k]);
        baked.applyMatrix4(headInverse);const original=baked.toArray();const fitted=original.map((v,c)=>identical?v:v*scale[c]+offset[c]);
        const distance=Math.hypot(...fitted.map((v,c)=>v-original[c]));if(distance>maximumDelta){maximumDelta=distance;maximumVertex={sourceNode:srcNode.getName(),primitive:pi,index:i,headLocal:original,delta:fitted.map((v,c)=>v-original[c])};}
        data.set(fitted,at*3);uvData.set(uv.getElement(i,[]),at*2);
      }
      for(const semantic of copied.listSemantics())copied.setAttribute(semantic,null);
      copied.setAttribute('POSITION',doc.createAccessor().setType(Accessor.Type.VEC3).setArray(data).setBuffer(doc.getRoot().listBuffers()[0]));
      copied.setAttribute('TEXCOORD_0',doc.createAccessor().setType(Accessor.Type.VEC2).setArray(uvData).setBuffer(doc.getRoot().listBuffers()[0]));
      copied.setIndices(doc.createAccessor().setType(Accessor.Type.SCALAR).setArray(Uint16Array.from(indices)).setBuffer(doc.getRoot().listBuffers()[0]));mesh.addPrimitive(copied);
      records.push({node:srcNode.getName(),primitive:pi,material:src.getMaterial().getExtras().originalName,vertices:map.size,triangles:indices.length/3});
    }
  }
  const contract={schemaVersion:1,style:'fantasy',hair:id,base,attachTo:HEAD,coordinateSpace:'Head-local',static:true,rootIdentity:true};doc.getRoot().setExtras({characterMaker:contract});
  return finish(doc,`hair-${base}-${id}.glb`,{kind:'hair',hair:id,base,label:HAIRS[id].label,source:HAIRS[id].source,contract,
    fit:{method:'Head-local affine correspondence of original back-scalp bounds; no scalp geometry is removed or hidden',donorBounds:from,targetBounds:to,scale:identical?[1,1,1]:scale,offset:identical?[0,0,0]:offset,maximumDeltaMeters:maximumDelta,maximumVertex},
    originalParts:records,physics:'Static rest bake. Original hair bones, skin weights and VRM spring-bone settings remain in the immutable source VRM; runtime secondary motion is not claimed.',
  });
}

if(args.includes('--import'))await importSources(argument('--from')??'/tmp/maker-vroid-probe-zcqd17j1');
await verifySources();await fs.mkdir(OUT,{recursive:true});
let prior={assets:[]};try{prior=JSON.parse(await fs.readFile(path.join(OUT,'manifest.json'),'utf8'));}catch(e){if(e.code!=='ENOENT')throw e;}
const entries=new Map(prior.assets.map(a=>[a.file,a]));
if(!args.includes('--skip-bodies'))for(const base of Object.keys(BASES)){const e=await buildBody(base);entries.set(e.file,e);}
for(const hair of (argument('--hair')??Object.keys(HAIRS).join(',')).split(',')){assert(HAIRS[hair]);for(const base of Object.keys(BASES)){const e=await buildHair(hair,base);entries.set(e.file,e);}}
const manifest={schemaVersion:1,version:'fantasy-v1',sourceLicense:'CC0-1.0',builder:path.relative(ROOT,fileURLToPath(import.meta.url)),builderSha256:sha(await fs.readFile(fileURLToPath(import.meta.url))),sourceManifestSha256:sha(await fs.readFile(path.join(SOURCES,'provenance.json'))),textureFinish:TEXTURE_FINISH,morphNames:MORPHS,hairIds:Object.keys(HAIRS),assets:[...entries.values()].sort((a,b)=>a.file.localeCompare(b.file))};
await fs.writeFile(path.join(OUT,'manifest.json'),JSON.stringify(manifest,null,2)+'\n');

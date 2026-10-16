import test from 'node:test';
import assert from 'node:assert/strict';
import { createDraftWriter } from './draft-writer.ts';
function fixture() {
  let ready = true, snapshot = { document: { name: 'Nami', value: 0 }, savedId: 'A', savedUpdatedAt: 'revision-1' }, serial = 0, now = 0, fail = false;
  const timers = new Map(), writes = [], errors = [];
  const writer = createDraftWriter({
    getSnapshot: () => ready && snapshot.document.name.trim() ? snapshot : null,
    write: value => { if (fail) throw Error('storage blocked'); writes.push(structuredClone(value)); },
    setTimer: (fn, delay) => { const id = ++serial; timers.set(id, { fn, at: now + delay }); return id; },
    clearTimer: id => timers.delete(id), onError: e => errors.push(e.message),
  });
  return { writer, writes, errors, timers, setReady: value => { ready = value; }, setFail: value => { fail = value; },
    set: value => { snapshot = value; }, edit: value => { snapshot = { ...snapshot, document: { ...snapshot.document, value } }; writer.changed(); },
    tick: ms => { now += ms; for (const [id, timer] of [...timers]) if (timer.at <= now) { timers.delete(id); timer.fn(); } },
  };
}
test('edit then immediate pagehide flush writes latest state before 400ms and cancels timer', () => { const f=fixture(); f.edit(7); f.tick(1); f.writer.flush(); assert.equal(f.writes[0].document.value,7); assert.equal(f.timers.size,0); f.tick(1000); assert.equal(f.writes.length,1); });
test('construction, initial hydration, visibility/pagehide and StrictMode cleanup do not write', () => { const f=fixture(); f.setReady(false); f.writer.flush(); f.writer.cancel(); f.setReady(true); f.writer.flush(); f.tick(1000); assert.equal(f.writes.length,0); });
test('normal edits remain debounced and coalesce to the newest document', () => { const f=fixture(); f.edit(1); f.tick(300); f.edit(2); f.tick(399); assert.equal(f.writes.length,0); f.tick(1); assert.equal(f.writes.length,1); assert.equal(f.writes[0].document.value,2); });
test('repeated lifecycle events after a successful flush do not duplicate writes', () => { const f=fixture(); f.edit(1); f.writer.flush(); f.writer.flush(); assert.equal(f.writes.length,1); assert.equal(f.writer.hasPending(),false); });
test('latest saved ID and revision are read at flush time, not timer creation', () => { const f=fixture(); f.edit(1); f.set({document:{name:'Copy',value:9},savedId:'B',savedUpdatedAt:'revision-8'}); f.writer.flush(); assert.deepEqual(f.writes[0],{document:{name:'Copy',value:9},savedId:'B',savedUpdatedAt:'revision-8'}); });
test('successful explicit save/source switch cancels the old timer and lifecycle write', () => { const f=fixture(); f.edit(1); f.set({document:{name:'Imported',value:12},savedId:null,savedUpdatedAt:null}); f.writer.acceptPersisted(); f.tick(1000); f.writer.flush(); assert.equal(f.writes.length,0); });
test('failed explicit switch retries only its newest document and revision', () => { const f=fixture(); f.edit(1); f.set({document:{name:'New source',value:12},savedId:'B',savedUpdatedAt:'new-revision'}); f.writer.changed(); f.writer.flush(); assert.equal(f.writes.length,1); assert.equal(f.writes[0].savedId,'B'); assert.equal(f.writes[0].document.value,12); });
test('unmount cleanup only cancels and never writes a stale identity', () => { const f=fixture(); f.edit(1); f.writer.cancel(); f.tick(1000); assert.equal(f.writes.length,0); assert.equal(f.writer.hasPending(),true); });
test('storage failure stays pending and later lifecycle retry can succeed', () => { const f=fixture(); f.edit(1); f.setFail(true); f.writer.flush(); assert.equal(f.errors.length,1); assert.equal(f.writer.hasPending(),true); f.setFail(false); f.writer.flush(); assert.equal(f.writes.length,1); assert.equal(f.writer.hasPending(),false); });
test('not-yet-hydrated or invalid-name snapshots are not written', () => { const f=fixture(); f.setReady(false); f.edit(1); f.writer.flush(); assert.equal(f.writes.length,0); f.setReady(true); f.set({document:{name:' ',value:2},savedId:'A',savedUpdatedAt:'revision-1'}); f.writer.changed(); f.writer.flush(); assert.equal(f.writes.length,0); f.set({document:{name:'Valid',value:3},savedId:'A',savedUpdatedAt:'revision-1'}); f.writer.changed(); f.writer.flush(); assert.equal(f.writes[0].document.value,3); });
test('a new edit during a synchronous write is not marked persisted by that older write', () => { let n=0; const timers=new Map(); let id=0; let writer; writer=createDraftWriter({getSnapshot:()=>n,write:()=>{if(n===0){n=1;writer.changed();}},setTimer:f=>{timers.set(++id,f);return id;},clearTimer:i=>timers.delete(i),onError:e=>{throw e;}});writer.changed();writer.flush();assert.equal(writer.hasPending(),true);writer.flush();assert.equal(writer.hasPending(),false); });

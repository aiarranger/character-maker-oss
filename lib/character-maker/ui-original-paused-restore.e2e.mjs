// Actual rendered pose comparison across paused save/reload/import; owned Chrome only.
// オリジナルの編集画面は Character Studio（/character-maker）で開く（tools/lib/studio-e2e.mjs）。
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import sharp from 'sharp';
import { importPlaywright } from '../../tools/lib/paths.mjs';
import { ORIGINAL_MOTIONS } from './original-config.ts';
import { catalogEntry, category, current, exportFile, importJson, installReadTool, inspector, openStudio, pause as pauseMotion, ready as studioReady, save, selectOriginal, waitConfig } from '../../tools/lib/studio-e2e.mjs';
const out = process.argv[2] ?? '/tmp/maker-closed-hand/paused-restore';
const ids = process.argv.find(v => v.startsWith('--ids='))?.slice(6).split(',') ?? ['generic-human-a', 'generic-human-b'];
// 動作の名前は画面と同じ（武器が選べるようになり「剣を構える」は「武器を構える」になった）。
const motions = ORIGINAL_MOTIONS;
await fs.mkdir(out,{recursive:true});
const { chromium } = await importPlaywright();
const browser = await chromium.launch({channel:'chrome',headless:true});
const context = await browser.newContext({viewport:{width:1440,height:960}});
await installReadTool(context);
const page = await context.newPage();page.setDefaultTimeout(30000);
const results=process.argv.includes('--resume') ? JSON.parse(await fs.readFile(path.join(out,'progress.json'),'utf8')).results : [],errors=[];page.on('pageerror',e=>errors.push(e.message));
const ready=()=>studioReady(page);
const config=async()=>(await current(page)).config;
const group=()=>inspector(page).getByRole('group',{name:'動作',exact:true});
const pause=()=>pauseMotion(page);
async function snapshot(label){
 await page.evaluate(()=>window.scrollTo(0,0));
 // 読み込み後の通知は舞台の高さを変えるので閉じる。
 const notice=page.getByRole('button',{name:'通知を閉じる',exact:true});if(await notice.count())await notice.click();
 // カメラは再生中の姿勢を重ねた範囲に合わせる（OriginalCharacterScene の envelope）。止めた姿勢だけで構図を決め直してから比べる。
 await page.getByRole('group',{name:'見る方向',exact:true}).getByRole('button',{name:'正面',exact:true}).click();
 await page.waitForTimeout(120);
 const frame=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>{
  const c=document.querySelector('[aria-label="キャラクターの3Dプレビュー"] canvas');resolve(c.toDataURL('image/png'));
 }))));
 // 停止中でも、詳細パネルの開閉や項目の切り替え直後はカメラが寄っている途中なので、2回続けて同じ絵になるまで待つ。
 let data=await frame();
 for(let i=0;i<40;i++){await page.waitForTimeout(100);const next=await frame();if(next===data)break;data=next;}
 const bytes=Buffer.from(data.split(',')[1],'base64');await fs.writeFile(path.join(out,label+'.png'),bytes);
 await page.screenshot({path:path.join(out,label+'.jpg'),type:'jpeg',quality:93});
 const raw=await sharp(bytes).ensureAlpha().raw().toBuffer({resolveWithObject:true});let count=0;
 for(let i=3;i<raw.data.length;i+=4)if(raw.data[i]>240)count++;
 assert(count>1000,'Actual character canvas must be painted');return {...raw,count};
}
function compare(a,b,label){assert.equal(a.info.width,b.info.width);assert.equal(a.info.height,b.info.height);let maskDiff=0,union=0,rgb=0,n=0;
 for(let i=0;i<a.data.length;i+=4){const aa=a.data[i+3]>128,bb=b.data[i+3]>128;if(aa||bb){union++;if(aa!==bb)maskDiff++;if(aa&&bb){for(let k=0;k<3;k++){rgb+=Math.abs(a.data[i+k]-b.data[i+k]);n++;}}}}
 const result={label,baselinePainted:a.count,restoredPainted:b.count,silhouetteDifference:maskDiff/union,meanRgbDifference:rgb/n};
 assert(result.silhouetteDifference<.008,`${label}: rendered pose / sword silhouette changed ${result.silhouetteDifference}`);
 assert(result.meanRgbDifference<1.5,`${label}: rendered character colors/pose changed ${result.meanRgbDifference}`);return result;
}
try{
 await openStudio(page);
 for(const id of ids){
  const ledger=catalogEntry(id);
  await selectOriginal(page,id);
  await category(page,'動き');
  for(const label of ['自動でまばたき','会話の口の動き','髪を揺らす']){const box=page.getByRole('checkbox',{name:label,exact:true});if(await box.count())await box.uncheck();}
  for(const[motion,label]of motions){
   if(results.some(r=>r.id===id&&r.motion===motion))continue;
   await group().getByRole('button',{name:label,exact:true}).click();await pause();await category(page,'動き');
   const progress=page.getByRole('slider',{name:'再生位置',exact:true});const box=await progress.boundingBox();await progress.click({position:{x:box.width/2,y:box.height/2}});
   await page.waitForFunction(()=>{const v=Number(document.querySelector('input[aria-label="再生位置"]').value);return v>.4&&v<.6;});
   await progress.press('Home');
   await page.waitForFunction(()=>Number(document.querySelector('input[aria-label="再生位置"]').value)===0);
   const prefix=`${id}-${motion}`, baseline=await snapshot(prefix+'-baseline');
   const name=`${ledger.name}-${motion}-停止復元`;
   await save(page,name);
   const expected=await config();assert.equal(expected.motion,motion);assert.equal(expected.playing,false);
   const file=await exportFile(page,'編集設定（JSON）',path.join(out,prefix+'.json'));
   await page.reload({waitUntil:'domcontentloaded',timeout:120000});await ready();assert.deepEqual(await config(),expected);
   await category(page,'動き'); // 再読み込みで閉じた詳細パネルを開き、基準と同じ舞台の幅で比べる
   const reloaded=await snapshot(prefix+'-reloaded');const reloadComparison=compare(baseline,reloaded,prefix+' reload');
   await category(page,'動き');await group().getByRole('button',{name:motion==='idle'?'武器を構える':'待機',exact:true}).click();await pause();
   await importJson(page,file);
   assert.equal(await page.getByRole('alertdialog').count(),0,'JSON switches without a confirmation');await ready();
   await waitConfig(page,{motion,playing:false});
   assert.deepEqual(await config(),expected);const imported=await snapshot(prefix+'-imported');const importComparison=compare(baseline,imported,prefix+' import');
   results.push({id,motion,playing:false,baselinePhase:0,reloadComparison,importComparison,images:['baseline','reloaded','imported'].map(s=>path.join(out,prefix+'-'+s+'.jpg'))});
   await fs.writeFile(path.join(out,'progress.json'),JSON.stringify({results,errors},null,2));console.log(JSON.stringify(results.at(-1)));
  }
 }
 assert.deepEqual(errors,[]);await fs.writeFile(path.join(out,'result.json'),JSON.stringify({success:true,method:'Real UI controls and native canvas pixels. Save does not retain seek time: baseline and restored motion are compared at phase zero. Visibility is included in the full rendered silhouette comparison.',results,errors},null,2));
}catch(error){await fs.writeFile(path.join(out,'failure.json'),JSON.stringify({error:String(error),results,errors},null,2));await page.screenshot({path:path.join(out,'failure.jpg'),type:'jpeg',quality:95}).catch(()=>{});throw error;}
finally{await context.close();await browser.close();}

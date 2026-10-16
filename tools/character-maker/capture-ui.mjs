// Capture the real editor in an independent, unauthenticated browser.
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { importPlaywright, MAKER_URL } from '../lib/paths.mjs';
const output = process.argv[2] ?? '/tmp/character-maker-design-review';
await mkdir(output, { recursive: true });
const { chromium } = await importPlaywright();
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
page.setDefaultTimeout(30000);
const ready = async () => { await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b=>b.textContent==='GLBを書き出す'&&!b.disabled),null,{timeout:120000}); };
const capture = async (name, settle = 700) => {
 await page.evaluate(()=>window.scrollTo(0,0));
 await page.evaluate(()=>document.fonts.ready);
 await page.waitForFunction(()=>[...document.images].every(img=>img.complete && img.naturalWidth > 0));
 await page.waitForTimeout(settle);
 await page.screenshot({path:join(output,name+'.jpg'),type:'jpeg',quality:94});
};
try {
 await page.goto(`${MAKER_URL}/character-maker/classic`);await ready();
 await page.getByRole('textbox',{name:'キャラクターの名前',exact:true}).fill('ルミナ');
 await page.getByRole('button',{name:'保存',exact:true}).click();
 await page.getByRole('button',{name:'通知を閉じる',exact:true}).click();
 await page.waitForTimeout(1000);await capture('desktop-1440');
 await page.setViewportSize({width:1257,height:712});await capture('desktop-1257');
 await page.getByRole('navigation',{name:'変更するパーツ'}).getByRole('button',{name:/^髪型/}).click();
 await capture('desktop-hair');
 await page.getByRole('button',{name:'全身',exact:true}).click();await capture('desktop-full');
 await page.getByRole('button',{name:'剣を構える',exact:true}).click();await page.waitForTimeout(500);await capture('desktop-sword-stance');
 await page.getByRole('button',{name:'剣を振る',exact:true}).click();await page.waitForTimeout(450);await capture('desktop-sword-attack',0);
 await page.getByRole('button',{name:'待機',exact:true}).click();await page.getByRole('button',{name:'顔',exact:true}).click();
 await page.setViewportSize({width:390,height:844});await capture('mobile-390');
 await page.screenshot({path:join(output,'mobile-full.jpg'),type:'jpeg',quality:94,fullPage:true});
 await page.evaluate(()=>window.scrollTo(0,350));
 await page.waitForTimeout(700);
 await page.getByRole('button',{name:'レイヤーショート',exact:true}).click();await ready();
 await page.waitForTimeout(700);
 await page.screenshot({path:join(output,'mobile-editing.jpg'),type:'jpeg',quality:94});
 await page.setViewportSize({width:1440,height:900});
 await page.getByRole('button',{name:/^アメコミ/}).click();await page.waitForTimeout(100);await ready();await capture('comic-1440');
 console.log(output);
} finally {await page.close({runBeforeUnload:false});await browser.close();}

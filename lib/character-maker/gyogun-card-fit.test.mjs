import assert from 'node:assert/strict';
import test from 'node:test';
import { gyogunCardFit } from './gyogun-card-fit.ts';

// 防ぐ故障: 窓の大きさによって入口のカードが消え、バーの黒いボタンに替わる（Owner 2026-11-16「幅で出さないのはやめて常に出す」）
test('the card is always on the stage, sized to the room it has', () => {
  // ステージの大きさは、窓 1440x900・1280x720・1024x768・1000x889・768x1024・1280x560・390x844・844x390 のときの実測
  assert.deepEqual(gyogunCardFit(852, 844, 'full'), { mode: 'card', width: 220 });
  assert.deepEqual(gyogunCardFit(728, 664, 'full'), { mode: 'card', width: 205 });
  assert.deepEqual(gyogunCardFit(628, 712, 'full'), { mode: 'card', width: 146 });
  assert.deepEqual(gyogunCardFit(604, 833, 'full'), { mode: 'card', width: 132 });
  assert.deepEqual(gyogunCardFit(768, 512, 'full'), { mode: 'card', width: 133 });
  assert.equal(gyogunCardFit(728, 504, 'full').mode, 'card');
  assert.deepEqual(gyogunCardFit(390, 439, 'full'), { mode: 'card', width: 132 });
  assert.deepEqual(gyogunCardFit(439, 346, 'full'), { mode: 'card', width: 132 });
  // ステージの左右の余白より広くしない
  assert.deepEqual(gyogunCardFit(180, 400, 'full'), { mode: 'card', width: 112 });
  // 顔の表示は、どの大きさでも動画を畳んだ札
  assert.deepEqual(gyogunCardFit(852, 844, 'face'), { mode: 'pill', width: 152 });
  assert.deepEqual(gyogunCardFit(390, 439, 'face'), { mode: 'pill', width: 152 });
});

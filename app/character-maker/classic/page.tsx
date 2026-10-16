import CharacterMaker from '../../../components/character-maker/CharacterMaker';

// 旧キャラメイク画面。aisns が埋め込むのと同じ形（オリジナルなし）で開く。
// オリジナルキャラ（ハル・ナミ・ルクス・モカ）の編集は Character Studio（/character-maker）で全員から選べる。
export default function Page() {
  return <CharacterMaker originals={[]} />;
}

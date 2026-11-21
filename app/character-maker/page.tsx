import CharacterStudio from '../../components/character-maker/CharacterStudio';

// Character Studio の本体は components/character-maker/CharacterStudio.tsx（yukikau.jp などへの埋め込みと共通）。
// このページは同じ場所で魚群繚乱（/gyogun/）と魚々娘（/gyogyo/）を配っているので、「魚群繚乱で遊ぶ」「魚々娘で遊ぶ」を出す（docs/gyogun-spec.md・docs/gyogyo-spec.md）。
export default function Page() {
  return <CharacterStudio games />;
}

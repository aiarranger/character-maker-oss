// Fish Kit の加工済み GLB から、魚だけが使う頂点属性を読む。座標と回転はキットの元の単位で扱う。
use bytemuck::{Pod, Zeroable};
use character_actor::glb::Primitive;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Parts {
    pub fin_jaw: [f32; 4],
    pub pivot_height: [f32; 4],
    pub flutter: [f32; 3],
}

/// 共通 GLB ローダーで解いた属性。異なる頂点数の模型を黙って組み合わせない。
fn attribute<'a, const N: usize>(p: &'a Primitive, name: &str) -> Result<&'a [[f32; N]], String> {
    let (width, values) = p.custom.get(name).ok_or_else(|| format!("魚の頂点属性 {name} がありません"))?;
    if *width != N || values.len() != p.positions.len() * N { return Err(format!("魚の頂点属性 {name} の長さが不正です")); }
    Ok(values.as_chunks::<N>().0)
}

pub fn vertices(p: &Primitive) -> Result<Vec<Parts>, String> {
    let fin = attribute::<3>(p, "_FIN")?;
    let piv = attribute::<3>(p, "_PIV")?;
    let jaw = attribute::<2>(p, "_JAW")?;
    let flutter = attribute::<3>(p, "_FLUTTER")?;
    let height = attribute::<1>(p, "_FINH")?;
    let count = p.positions.len();
    Ok((0..count).map(|i| Parts {
        fin_jaw: [fin[i][0], fin[i][1], fin[i][2], jaw[i][0]],
        pivot_height: [piv[i][0], piv[i][2], -piv[i][1], height[i][0]],
        flutter: flutter[i],
    }).collect())
}

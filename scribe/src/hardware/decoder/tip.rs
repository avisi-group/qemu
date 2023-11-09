struct Parser<'buf> {
    buf: &'buf [u8],
}

impl<'buf> Parser<'buf> {
    pub fn new(buf: &'buf [u8]) -> Self {
        Self { buf }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// Payload: 16 bits. Update last IP
    Update16,
    /// Payload: 32 bits. Update last IP
    Update32,
    /// Payload: 48 bits. Update last IP
    Update48,
    /// Payload: 64 bits. Full address
    Update64,
    /// Payload: 48 bits. Sign extend to full address
    SignExtend48,
    /// Full address, but do not emit a PC
    Update64NoEmit,
}

impl<'buf> Iterator for Parser<'buf> {
    type Item = (Kind, u64);

    fn next(&mut self) -> Option<Self::Item> {
        None
    }
}

// #[cfg(test)]
// mod tests {
//     use {
//         crate::hardware::decoder::tip::{Kind, Parser},
//         libipt::{
//             packet::{Packet, PacketDecoder},
//             ConfigBuilder,
//         },
//     };

//     #[test]
//     fn equals_ipt() {
//         let data = include_bytes!("../../../benches/tip.raw");

//         let expected = {
//             let mut decoder =
//                 PacketDecoder::new(&ConfigBuilder::new(&mut data.to_owned()).unwrap().finish())
//                     .unwrap();
//             decoder.sync_forward().unwrap();
//             assert_eq!(decoder.sync_offset().unwrap(), 0);

//             decoder
//                 .map(|r| {
//                     r.map(|p| match p {
//                         Packet::Tip(inner) => {
//                             let compression = match inner.compression() {
//                                 libipt::packet::Compression::Suppressed => return None,
//                                 libipt::packet::Compression::Update16 => Kind::Update16,
//                                 libipt::packet::Compression::Update32 => Kind::Update32,
//                                 libipt::packet::Compression::Sext48 => Kind::SignExtend48,
//                                 libipt::packet::Compression::Update48 => Kind::Update48,
//                                 libipt::packet::Compression::Full => Kind::Update64,
//                             };
//                             Some((compression, inner.tip()))
//                         }
//                         Packet::Fup(inner) => Some((Kind::Update64NoEmit, inner.fup())),
//                         _ => None,
//                     })
//                 })
//                 .filter_map(Result::transpose)
//                 .collect::<Result<Vec<_>, _>>()
//                 .unwrap()
//         };

//         let got = Parser::new(data).collect::<Vec<_>>();

//         assert_eq!(expected, got);
//     }
// }

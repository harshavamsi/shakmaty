//! Zobrist hashing for positions.
//!
//! # Examples
//!
//! ```
//! use shakmaty::Chess;
//! use shakmaty::zobrist::ZobristHash;
//!
//! let pos = Chess::default();
//! assert_eq!(pos.zobrist_hash::<u64>(), 0x463b96181691fc9c);
//! ```

use crate::{
    Bitboard, Board, ByColor, Castles, CastlingMode, CastlingSide, Color, FromSetup,
    Material, Move, MoveList, Outcome, Piece, Position, PositionError, RemainingChecks, Role,
    Setup, Square,
    Chess,
    File,
};
use std::ops::BitXorAssign;
use std::num::NonZeroU32;

/// Integer type that can be returned as a Zobrist hash.
pub trait ZobristValue: BitXorAssign + Default {
    fn zobrist_for_piece(square: Square, piece: Piece) -> Self;
    fn zobrist_for_white_turn() -> Self;
    fn zobrist_for_castling_right(color: Color, side: CastlingSide) -> Self;
    fn zobrist_for_en_passant_file(file: File) -> Self;
    fn zobrist_for_remaining_checks(color: Color, remaining: u8) -> Self;
    fn zobrist_for_promoted(square: Square) -> Self;
    fn zobrist_for_pocket(color: Color, role: Role, pieces: u8) -> Self;
}

macro_rules! zobrist_value_impl {
    ($($t:ty)+) => {
        $(impl ZobristValue for $t {
            fn zobrist_for_piece(square: Square, piece: Piece) -> $t {
                let piece_idx = (usize::from(piece.role) - 1) * 2 + piece.color as usize;
                PIECE_MASKS[64 * piece_idx + usize::from(square)] as $t
            }

            fn zobrist_for_white_turn() -> $t {
                WHITE_TURN_MASK as $t
            }

            fn zobrist_for_castling_right(color: Color, side: CastlingSide) -> $t {
                CASTLING_RIGHT_MASKS[match (color, side) {
                    (Color::White, CastlingSide::KingSide) => 0,
                    (Color::White, CastlingSide::QueenSide) => 1,
                    (Color::Black, CastlingSide::KingSide) => 2,
                    (Color::Black, CastlingSide::QueenSide) => 3,
                }] as $t
            }

            fn zobrist_for_en_passant_file(file: File) -> $t {
                EN_PASSANT_FILE_MASKS[usize::from(file)] as $t
            }

            fn zobrist_for_remaining_checks(color: Color, remaining: u8) -> $t {
                if remaining < 3 {
                    REMAINING_CHECKS_MASKS[usize::from(remaining) + color.fold(0, 3)] as $t
                } else {
                    <$t>::default()
                }
            }

            fn zobrist_for_promoted(square: Square) -> $t {
                PROMOTED_MASKS[usize::from(square)] as $t
            }

            fn zobrist_for_pocket(color: Color, role: Role, pieces: u8) -> $t {
                if pieces > 0 {
                    POCKET_MASKS[usize::from(pieces - 1)] as $t // TODO
                } else {
                    <$t>::default()
                }
            }
        })+
    }
}

zobrist_value_impl! { u8 u16 u32 u64 u128 }

/// Supports Zobrist hashing.
pub trait ZobristHash {
    /// Computes the Zobrist hash from scratch.
    fn zobrist_hash<V: ZobristValue>(&self) -> V;

    /// Prepares an incremental update of the Zobrist hash before playing move
    /// `m` in `self`. Returns a new intermediate Zobrist hash, or `None`
    /// if incremental updating is not supported.
    fn prepare_incremental_zobrist_hash<V: ZobristValue>(&self, previous: V, m: &Move) -> Option<V> {
        None
    }

    /// Finalizes an incremental update of the Zobrist hash after playing move
    /// `m` in `self`. Returns the new Zobrist hash, or `None` if incremental
    /// updating is not supported.
    fn finalize_incremental_zobrist_hash<V: ZobristValue>(&self, intermediate: V, m: &Move) -> Option<V> {
        None
    }
}

impl ZobristHash for Chess {
    fn zobrist_hash<V: ZobristValue>(&self) -> V { hash_position(self) }
}

#[cfg(feature = "variant")]
mod variant {
    impl ZobristHash for Antichess {
        fn zobrist_hash<V: ZobristValue>(&self) -> V { hash_position(self) }
    }

    impl ZobristHash for Atomic {
        fn zobrist_hash<V: ZobristValue>(&self) -> V { hash_position(self) }
    }

    impl ZobristHash for Horde {
        fn zobrist_hash<V: ZobristValue>(&self) -> V { hash_position(self) }
    }

    impl ZobristHash for KingOfTheHill {
        fn zobrist_hash<V: ZobristValue>(&self) -> V { hash_position(self) }
    }

    impl ZobristHash for RacingKings {
        fn zobrist_hash<V: ZobristValue>(&self) -> V { hash_position(self) }
    }
}

/// A wrapper for [`Position`] that maintains an incremental Zobrist hash.
#[derive(Debug, Clone)]
pub struct Zobrist<P, V> {
    pos: P,
    zobrist: Option<V>,
}

impl<P, V> Zobrist<P, V> {
    pub fn new(pos: P) -> Zobrist<P, V> {
        Zobrist {
            pos,
            zobrist: None,
        }
    }

    pub fn into_inner(self) -> P {
        self.pos
    }
}

impl<P: Default, V> Default for Zobrist<P, V> {
    fn default() -> Zobrist<P, V> {
        Self::new(P::default())
    }
}

impl<P: FromSetup + Position, V> FromSetup for Zobrist<P, V> {
    fn from_setup(setup: &dyn Setup, mode: CastlingMode) -> Result<Self, PositionError<Self>> {
        match P::from_setup(setup, mode) {
            Ok(pos) => Ok(Zobrist::new(pos)),
            Err(err) => Err(PositionError {
                pos: Zobrist::new(err.pos),
                errors: err.errors,
            }),
        }
    }
}

impl<P: Setup, V> Setup for Zobrist<P, V> {
    fn board(&self) -> &Board { self.pos.board() }
    fn promoted(&self) -> Bitboard { self.pos.promoted() }
    fn pockets(&self) -> Option<&Material> { self.pos.pockets() }
    fn turn(&self) -> Color { self.pos.turn() }
    fn castling_rights(&self) -> Bitboard { self.pos.castling_rights() }
    fn ep_square(&self) -> Option<Square> { self.pos.ep_square() }
    fn remaining_checks(&self) -> Option<&ByColor<RemainingChecks>> { self.pos.remaining_checks() }
    fn halfmoves(&self) -> u32 { self.pos.halfmoves() }
    fn fullmoves(&self) -> NonZeroU32 { self.pos.fullmoves() }
}

impl<P: Position + ZobristHash, V: ZobristValue> Position for Zobrist<P, V> {
    fn legal_moves(&self) -> MoveList { self.pos.legal_moves() }
    fn san_candidates(&self, role: Role, to: Square) -> MoveList { self.pos.san_candidates(role, to) }
    fn castling_moves(&self, side: CastlingSide) -> MoveList { self.pos.castling_moves(side) }
    fn en_passant_moves(&self) -> MoveList { self.pos.en_passant_moves() }
    fn capture_moves(&self) -> MoveList { self.pos.capture_moves() }
    fn promotion_moves(&self) -> MoveList { self.pos.promotion_moves() }
    fn is_irreversible(&self, m: &Move) -> bool { self.pos.is_irreversible(m) }
    fn king_attackers(&self, square: Square, attacker: Color, occupied: Bitboard) -> Bitboard { self.pos.king_attackers(square, attacker, occupied) }
    fn castles(&self) -> &Castles { self.pos.castles() }
    fn is_variant_end(&self) -> bool { self.pos.is_variant_end() }
    fn has_insufficient_material(&self, color: Color) -> bool { self.pos.has_insufficient_material(color) }
    fn variant_outcome(&self) -> Option<Outcome> { self.pos.variant_outcome() }

    fn play_unchecked(&mut self, m: &Move) {
        self.zobrist = self.zobrist.take().and_then(|value| self.pos.prepare_incremental_zobrist_hash(value, m));
        self.pos.play_unchecked(m);
        self.zobrist = self.zobrist.take().and_then(|value| self.pos.finalize_incremental_zobrist_hash(value, m));
    }
}

fn hash_board<V: ZobristValue>(board: &Board) -> V {
    let mut zobrist = V::default();
    for (sq, piece) in board.pieces() {
        zobrist ^= V::zobrist_for_piece(sq, piece);
    }
    zobrist
}

fn hash_position<P: Position, V: ZobristValue>(pos: &P) -> V {
    let mut zobrist = hash_board(pos.board());

    if pos.turn() == Color::White {
        zobrist ^= V::zobrist_for_white_turn();
    }

    let castles = pos.castles();
    if castles.has(Color::White, CastlingSide::KingSide) {
        zobrist ^= V::zobrist_for_castling_right(Color::White, CastlingSide::KingSide);
    }
    if castles.has(Color::White, CastlingSide::QueenSide) {
        zobrist ^= V::zobrist_for_castling_right(Color::White, CastlingSide::QueenSide);
    }
    if castles.has(Color::Black, CastlingSide::KingSide) {
        zobrist ^= V::zobrist_for_castling_right(Color::Black, CastlingSide::KingSide);
    }
    if castles.has(Color::Black, CastlingSide::QueenSide) {
        zobrist ^= V::zobrist_for_castling_right(Color::Black, CastlingSide::QueenSide);
    }

    if let Some(sq) = pos.ep_square() {
        zobrist ^= V::zobrist_for_en_passant_file(sq.file());
    }

    zobrist
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Chess;
    use crate::fen::Fen;

    #[test]
    #[ignore]
    fn test_polyglot() {
        let pos = Chess::default();
        assert_eq!(pos.zobrist_hash::<u64>(), 0x463b96181691fc9c);

        let pos: Chess = "rnbqkbnr/p1pppppp/8/8/P6P/R1p5/1P1PPPP1/1NBQKBNR b Kkq - 1 4"
            .parse::<Fen>()
            .expect("valid fen")
            .position(CastlingMode::Standard)
            .expect("legal position");
        assert_eq!(pos.zobrist_hash::<u64>(), 0x5c3f9b829b279560);
    }
}

const PIECE_MASKS: [u128; 64 * 6 * 2] = [
    0x9d39247e33776d4152b375aa7c0d7bac,
    0x2af7398005aaa5c7208d169a534f2cf5,
    0x44db0150246235478981513722b47f24,
    0x9c15f73e62a76ae209b8f20f910a8ff7,
    0x75834465489c0c890b8ea70255209cc0,
    0x3290ac3a203001bfa688a9791f027500,
    0x0fbbad1f6104227919b88b8ffaed8f55,
    0xe83a908ff2fb60ca88bf7822d00d5526,
    0x0d7e765d58755c10db7bf62ab390b71b,
    0x1a083822ceafe02d33a6ac1d85c9f22f,
    0x9605d5f0e25ec3b055ab2a27271d42ac,
    0xd021ff5cd13a2ed540a21ff9c803fca4,
    0x40bdf15d4a672e323c169aeb80a1d5d2,
    0x011355146fd5639587684e27293ecf96,
    0x5db4832046f3d9e5f8b91b39d4c6997c,
    0x239f8b2d7ff719cc1d5f744f312fd467,
    0x05d1a1ae85b49aa1eda18c452d5de5b4,
    0x679f848f6e8fc9717497db888eccda0f,
    0x7449bbff801fed0b94c1bb7016749887,
    0x7d11cdb1c3b7adf035b23d663606fde2,
    0x82c7709e781eb7cc17b4ae80b8184845,
    0xf3218f1c9510786c8bd98922a3089d8e,
    0x331478f3af51bbe6fec77fb07cea5e84,
    0x4bb38de5e7219443e153a54d23c93a8a,
    0xaa649c6ebcfd50fca196fa76c24405eb,
    0x8dbd98a352afd40b6f333e11d079240a,
    0x87d2074b81d7921723b8d480df5bb521,
    0x19f3c751d3e92ae1634adaec002b3000,
    0xb4ab30f062b19abf3d0e41d65872d549,
    0x7b0500ac42047ac45aba83908462b892,
    0xc9452ca81a09d85d26457864aff288af,
    0x24aa6c514da2750043f10561015da64e,
    0x4c9f34427501b447545cc6285df42807,
    0x14a68fd73c910841a7140dc7b82e96ef,
    0xa71b9b83461cbd93b1dcadc8fe30a8d4,
    0x03488b95b0f1850f72ebd048ba373ac4,
    0x637b2b34ff93c0402eb0ddf1351a1adb,
    0x09d1bc9a3dd90a949cdc8c44a201836d,
    0x3575668334a1dd3b0afc1fb45a728973,
    0x735e2b97a4c45a2358c8fa415b96ec95,
    0x18727070f1bd400b497a9b9a7f9f8872,
    0x1fcbacd259bf02e7bff840799ee05fdf,
    0xd310a7c2ce9b6555e4ec1554316c2704,
    0xbf983fe0fe5d82443c9f0c8b89f31f3e,
    0x9f74d14f7454a8244a601b99475baf4e,
    0x51ebdc4ab9ba30356c65e1386536c3a9,
    0x5c82c505db9ab0fab60a571d59e8a485,
    0xfcf7fe8a3430b241e23c5d7045696d85,
    0x3253a729b9ba3ddec9d4d61b569ec607,
    0x8c74c368081b3075ce9ed71a6d18deb2,
    0xb9bc6c87167c33e72dbc16559bdba870,
    0x7ef48f2b83024e2050cda5339d836c83,
    0x11d505d4c351bd7f98091ef4f2ab1ed3,
    0x6568fca92c76a243f5803ac17fc45ecf,
    0x4de0b0f40f32a7b809730ef15a78c687,
    0x96d693460cc37e5df8bb209d715ab566,
    0x42e240cb63689f2f0c5b201d6cb89a50,
    0x6d2bdcdae291966152571fbfabb4a367,
    0x42880b0236e4d9511b1db82269890861,
    0x5f0f4a5898171bb69423f70ed512f1ea,
    0x39f890f579f92f8879e448c72183e2a5,
    0x93c5b5f47356388b3c88a0cf5b852900,
    0x63dc359d8d231b789e12f819acaa6653,
    0xec16ca8aea98ad76c6f09266299a5902,
    0x5355f900c2a82dc73e8cad2210fce3f3,
    0x07fb9f855a997142a3868eff53346da1,
    0x5093417aa8a7ed5e61de5496186b0d70,
    0x7bcbc38da25a7f3ce17a09f0e53bc940,
    0x19fc8a768cf4b6d4e0ffe83afe44ec11,
    0x637a7780decfc0d9f35a5e3184c1c980,
    0x8249a47aee0e41f783390d9b2e7563a6,
    0x79ad695501e7d1e8950f14737ed6be5b,
    0x14acbaf4777d57766df42fcfa743809d,
    0xf145b6beccdea1950f2b1872ba3fef30,
    0xdabf2ac8201752fc04171b94f58c5d2e,
    0x24c3c94df9c8d3f678b05fea0dc77c38,
    0xbb6e2924f03912eaa76ddf41aa675504,
    0x0ce26c0b95c980d9e634bc8f87d0fe75,
    0xa49cd132bfbf7cc42dbf77a8851237de,
    0xe99d662af424393910f9b7d996836741,
    0x27e6ad7891165c3f26f6547bb0471fb0,
    0x8535f040b9744ff1f727c06db8bb34e0,
    0x54b3f4fa5f40d87328984171f866b615,
    0x72b12c32127fed2b349d245078c312ef,
    0xee954d3c7b411f4799f8f1ab94a13206,
    0x9a85ac909a24eaa1967d7e5e99566e67,
    0x70ac4cd9f04f21f5470fda103f9476cc,
    0xf9b89d3e99a075c237dad4fcdedc6db8,
    0x87b3e2b2b5c907b199f91b1cd65c50f0,
    0xa366e5b8c54f48b86d89c29cb7034aef,
    0xae4a9346cc3f7cf2824c9daa114a11c7,
    0x1920c04d47267bbdf3b1ee14505939c6,
    0x87bf02c6b49e2ae92fa0d39cbee05ced,
    0x092237ac237f3859c0c43ea8a642a49c,
    0xff07f64ef8ed14d04824a871bca34e17,
    0x8de8dca9f03cc54e394b500f07f96989,
    0x9c1633264db49c897c6efb4dc9bea9d9,
    0xb3f22c3d0b0b38edca09213bfeb36c6a,
    0x390e5fb44d01144b0069832f9f2bd0b5,
    0x5bfea5b4712768e9f092a01d0d4420da,
    0x1e1032911fa789846952e2015db39c5a,
    0x9a74acb964e78cb3f3993e2a4cdf615e,
    0x4f80f7a035dafb047d3ddc6bef2ed6f2,
    0x6304d09a0b3738c4a7040db38e233bac,
    0x2171e64683023a08a8b59fe4836ffc08,
    0x5b9b63eb9ceff80cc55dbb54360414a9,
    0x506aacf4898893423e24465359dc03c0,
    0x1881afc9a3a701d6d27a416ed84cc3b7,
    0x65030804407506447a77677e0de620c4,
    0xdfd395339cdbf4a730cacd32e0313d3b,
    0xef927dbcf00c20f2dc6952fd3e61c11a,
    0x7b32f7d1e03680ec08ab1642b5129e01,
    0xb9fd7620e7316243aa8e0962b8eebcdc,
    0x05a7e8a57db91b776dda36bacc3b2e2c,
    0xb5889c6e15630a75ba82f4e2cb60b43e,
    0x4a750a09ce9573f7509da4ba5295c4a5,
    0xcf464cec899a2f8a36a18fa38c3d74c6,
    0xf538639ce705b824b9e5652481a6df69,
    0x3c79a0ff5580ef7f5f4946b7dd41d1c7,
    0xede6c87f8477609dfd7a4fb7bfe1d23d,
    0x799e81f05bc93f3132e0b2b68ea83031,
    0x86536b8cf3428a8cf25fcb24f0c19623,
    0x97d7374c60087b73a317676dc1eb8797,
    0xa246637cff328532f754b557c09ae146,
    0x043fcae60cc0eba05bbd920fffe5fa7a,
    0x920e449535dd359e5b3c978e296d2280,
    0x70eb093b15b290ccca9c1ea34fc1484f,
    0x73a1921916591cbd470c408e3b3d2dc5,
    0x56436c9fe1a1aa8d6a0772093f97e152,
    0xefac4b70633b8f81fa76d36719e7e5e3,
    0xbb215798d45df7af2e799233a544062a,
    0x45f20042f24f1768e003451144a03be8,
    0x930f80f4e8eb7462974d8f4ee692ed35,
    0xff6712ffcfd75ea1d30afadfc4dc52f5,
    0xae623fd67468aa70278dde02bf30c1da,
    0xdd2c5bc84bc8d8fc8b7e3a2bf5a061a3,
    0x7eed120d54cf2dd9d1443752e511a579,
    0x22fe545401165f1cd1b02672a0ec44cf,
    0xc91800e98fb99929ba3005c8512514f1,
    0x808bd68e6ac103652e3b86211f6b4295,
    0xdec468145b7605f6057431bfeaa9d6f5,
    0x1bede3a3aef53302a348ddd66378afaf,
    0x43539603d6c556024a1817775b086ce1,
    0xaa969b5c691ccb7a184b1c983a6a1a77,
    0xa87832d392efee56cb70392e7b7db185,
    0x65942c7b3c7e11ae3b1b1166a648330e,
    0xded2d633cad004f6e5201b155f51cc30,
    0x21f08570f420e5654d053ee865f21b96,
    0xb415938d7da94e3cd8d8062e343d9c66,
    0x91b859e59ecb635013507b31a966da7d,
    0x10cff333e0ed804ad637536d2e7a58b0,
    0x28aed140be0bb7ddcbcae035eab824a0,
    0xc5cc1d89724fa4568c77fe1e1b06691b,
    0x5648f680f11a2741fd7841ed1ab4b961,
    0x2d255069f0b7dab3caa88da017669d53,
    0x9bc5a38ef729abd4abda5baf650b3675,
    0xef2f054308f6a2bcfe14d0c6b8f11e97,
    0xaf2042f5cc5c2858c13423f81c4b9adf,
    0x480412bab7f5be2a34507a3503935243,
    0xaef3af4a563dfe43ec504bd0c7ae79a1,
    0x19afe59ae451497f0bc761ea4004d2ae,
    0x52593803dff1e8403a0748078a78fd4d,
    0xf4f076e65f2ce6f0c5f36bde8caa93fe,
    0x11379625747d5af35b2299dc44080278,
    0xbce5d2248682c1153f99cbeb6ec653fa,
    0x9da4243de836994f48ebcfc004b524ca,
    0x066f70b33fe09017d2278829cd344d05,
    0x4dc4de189b671a1c5fe637e58fc1c0f3,
    0x51039ab7712457c30a4b136f25a65a32,
    0xc07a3f80c31fb4b44119314b520d04d9,
    0xb46ee9c5e64a6e7c5354a8b08947cc8e,
    0xb3819a42abe61c876001d6a94517300b,
    0x21a007933a522a2014597a074f133855,
    0x2df16f761598aa4fdc9a6baf92ffde03,
    0x763c4a1371b368fdc5cbc5270de986b0,
    0xf793c46702e086a095c72d49bd7560be,
    0xd7288e012aeb8d3112b437e4c286737a,
    0xde336a2a4bc1c44baa7c6f89f1442c5d,
    0x0bf692b38d079f231a3ebbf317bfc4d8,
    0x2c604a7a177326b34ad3c9fa863a5aa3,
    0x4850e73e03eb6064c7c94147de663b5b,
    0xcfc447f1e53c8e1b840e7fe4b35d4a4b,
    0xb05ca3f564268d998921109126e23341,
    0x9ae182c8bc9474e8a18a4f12c127de17,
    0xa4fc4bd4fc5558ca471973e4dc6efb4b,
    0xe755178d58fc4e76c723867b98d07330,
    0x69b97db1a4c03dfef5fcc6de350950d1,
    0xf9b5b7c4acc67c96b90913e02bde576a,
    0xfc6a82d64b8655fb5554c92f272b73c5,
    0x9c684cb6c4d244173738a3f0fdf5d9c6,
    0x8ec97d2917456ed03771f25e5e278ee3,
    0x6703df9d2924e97ea9d58a812b10906e,
    0xc547f57e42a7444e2814f2a19d8670eb,
    0x78e37644e7cad29e5ced6d617e9d6b4d,
    0xfe9a44e9362f05fa69be27bdc682e06a,
    0x08bd35cc38336615945e3cd54c7a41f4,
    0x9315e5eb3a129acefac825c29c4e52fc,
    0x94061b871e04df7595c380633671f3c0,
    0xdf1d9f9d784ba010b1f0f11b309f849f,
    0x3bba57b68871b59d36b7ac17862bc4ac,
    0xd2b7adeeded1f73f8b89835b5e731ac5,
    0xf7a255d83bc373f8122138b676fc6561,
    0xd7f4f2448c0ceb81ce3107b858b368ea,
    0xd95be88cd210ffa7aa14dd3733de4203,
    0x336f52f8ff4728e7ec4ea6805a8dad1e,
    0xa74049dac312ac71ce5cd5938049dcf0,
    0xa2f61bb6e437fdb521156227c06a4b0b,
    0x4f2a5cb07f6a35b3da13d541802c4d5e,
    0x87d380bda5bf7859fbf03d5c9f783bb2,
    0x16b9f7e06c453a21e512fa9fd5c68b8a,
    0x7ba2484c8a0fd54e30bd781b22277dcd,
    0xf3a678cad9a2e38c56625e22aef316ec,
    0x39b0bf7dde437ba2ed75251a71024db6,
    0xfcaf55c1bf8a4424b468dc45b39cde2f,
    0x18fcf680573fa59468b33836bea9a0a0,
    0x4c0563b89f495ac33187565f03cc0d85,
    0x40e087931a00930d9bbc591ddc43447f,
    0x8cffa9412eb642c1c53a29458191d2db,
    0x68ca39053261169f6bc263803d691ec8,
    0x7a1ee967d27579e204cca68628858bac,
    0x9d1d60e5076f5b6fa20a13cffa4679d1,
    0x3810e399b6f65ba285725a1e096e1abf,
    0x32095b6d4ab5f9b1bc986393043f78d5,
    0x35cab62109dd038a0fa47125507ccb12,
    0xa90b24499fcfafb1c2c27b60c8b2ce36,
    0x77a225a07cc2c6bd217520a809c97da6,
    0x513e5e634c70e331552ad48c96617c16,
    0x4361c0ca3f692f12758c0637401144ae,
    0xd941aca44b20a45bf1ae50d591aeb10f,
    0x528f7c8602c5807b0c127280b89240a3,
    0x52ab92beb9613989a9a8cd5ddd7737b0,
    0x9d1dfa2efc557f738506683f3e28c050,
    0x722ff175f572c3488105b0573483941f,
    0x1d1260a51107fe97d00bcf6974e8788c,
    0x7a249a57ec0c9ba23311a2a4e61fc638,
    0x04208fe9e8f7f2d65b31cba035ff4f50,
    0x5a110c6058b920a09ef049141a01e743,
    0x0cd9a497658a56983355b7a63e03cf20,
    0x56fd23c8f9715a4c78bf716c2f94ffcf,
    0x284c847b9d887aae232304d6a359676e,
    0x04feabfbbdb619cbffeebdd04f15816e,
    0x742e1e651c60ba83594fdc90c434a4fd,
    0x9a9632e65904ad3cba5cc088b72c0942,
    0x881b82a13b51b9e2036efdc30e389de2,
    0x506e6744cd9749245038b23c9af174d2,
    0xb0183db56ffc6a799b5e64f304474d48,
    0x0ed9b915c66ed37e280a4b8c73c2e8d8,
    0x5e11e86d5873d484fda076be88bcc507,
    0xf678647e3519ac6eafc896ae852c60c2,
    0x1b85d488d0f20cc5be903340939e63fd,
    0xdab9fe6525d890217a97bd60aba4c349,
    0x0d151d86adb73615f62e51f178597cf9,
    0xa865a54edcc0f0198f9ab42711b663dc,
    0x93c42566aef98ffbc8d003d119dcac63,
    0x99e7afeabe000731ef2101c32adaed38,
    0x48cbff086ddf285adde81906502ad1b0,
    0x7f9b6af1ebf78baf149756bc21368632,
    0x58627e1a149bba2125c80f323a516eaa,
    0x2cd16e2abd791e333ea039f7ff28ae8e,
    0xd363eff5f09779960caf481f40063dd8,
    0x0ce2a38c344a6eedbce23e106b1eefd7,
    0x1a804aadb9cfa74110853ea82a5ccb34,
    0x907f30421d78c5dee7c76ac3dbbf8c8c,
    0x501f65edb3034d071624c0ce1532313d,
    0x37624ae5a48fa6e95f3895b25d7b4744,
    0x957baf61700cff4efbe363cbb55a913e,
    0x3a6c27934e31188a35850e8f63400ddd,
    0xd49503536abca3453d300047b5ddde66,
    0x088e049589c432e01c1c7ca8b3386353,
    0xf943aee7febf21b8986ec52ac2c88cec,
    0x6c3b8e3e336139d3c93b616a554d23c8,
    0x364f6ffa464ee52e211d7b5759da7504,
    0xd60f6dcedc314222f2663fc59b541585,
    0x56963b0dca418fc0f57fefeadb21b029,
    0x16f50edf91e513af30fd60d9ee260966,
    0xef1955914b609f933c29da000d5b9a08,
    0x565601c0364e3228d0d6203fa69da0ba,
    0xecb53939887e81758167e4bd87c6f05e,
    0xbac7a9a18531294b5c063405c62f8154,
    0xb344c470397bba52b86fe57d53081fe6,
    0x65d34954daf3cebdeb60ad080cd573fe,
    0xb4b81b3fa97511e2bfbbb41602635b78,
    0xb422061193d6f6a74e39d536b723213c,
    0x071582401c38434d56d7e0468df15a47,
    0x7a13f18bbedc4ff59e601537348ed732,
    0xbc4097b116c524d2ee2e827d9faa74c1,
    0x59b97885e2f2ea28e43302e0d517a316,
    0x99170a5dc3115544f662e9ba781a1fae,
    0x6f423357e7c6a9f9e83da2efce442856,
    0x325928ee6e6f8794143ae097749a513a,
    0xd0e4366228b03343a203386e6a86f7c7,
    0x565c31f7de89ea2773905f8c5056ecee,
    0x30f56114841194140da07ce44c0142e4,
    0xd873db391292ed4f8b9e97003ef01d2e,
    0x7bd94e1d8e17debcd8c666a665840842,
    0xc7d9f16864a76e948bb1069bba169263,
    0x947ae053ee56e63c6bdc866d7daa19dc,
    0xc8c93882f9475f5fe1115bb3f8ad0cfe,
    0x3a9bf55ba91f81ca0859ae34a51ed77c,
    0xd9a11fbb3d9808e4f1d73663c53a0156,
    0x0fd22063edc29fca669283df212c93db,
    0xb3f256d8aca0b0b97489abc08bd4db15,
    0xb03031a8b4516e84f9d2b26d0375aab0,
    0x35dd37d5871448afde4856e7777e27d1,
    0xe9f6082b05542e4e2caeaf61386fa1f2,
    0xebfafa33d7254b597c6d4b00383f052a,
    0x9255abb50d532280b1943df6ea3687ff,
    0xb9ab4ce57f2d34f37e4d1baca94da20d,
    0x693501d62829755138d1a6b6448fdc40,
    0xc62c58f97dd949bf8aed53051756d212,
    0xcd454f8f19c5126a1805fa7482c60f4e,
    0xbbe83f4ecc2bdecb24c9891c2f4db0b1,
    0xdc842b7e2819e230bb703190b30eb664,
    0xba89142e007503b8b52f857f41e68fce,
    0xa3bc941d0a5061cbeb5a7a714d4ec1a1,
    0xe9f6760e32cd8021c41334a36d4211ea,
    0x09c7e552bc76492fe60188ecb537010d,
    0x852f54934da55cc9ff67dd932e5755cc,
    0x8107fccf064fcf5614c92be552467cfb,
    0x098954d51fff6580c94fb8eae42b3453,
    0x23b70edb1955c4bf0bd007042735acc6,
    0xc330de426430f69db9dd82debb9abd3d,
    0x4715ed43e8a45c0a3312b675a5bc5dfb,
    0xa8d7e4dab780a08dd2e9447e6c6d3509,
    0x0572b974f03ce0bbd4d771a89c91beb6,
    0xb57d2e985e1419c731d0724183248884,
    0xe8d9ecbe2cf3d73fe7229acdb568bc00,
    0x2fe4b17170e59750b4ef94e8251ecb66,
    0x11317ba87905e790ac817cad3cfb00ed,
    0x7fbf21ec8a1f45ec3fe869890b69552c,
    0x1725cabfcb045b0091aec362daa37fcd,
    0x964e915cd5e2b207fb25f88ca887ca77,
    0x3e2b8bcbf016d66d053bff886db0847c,
    0xbe7444e39328a0ac991fd5641b666e80,
    0xf85b2b4fbcde44b724fc37ad820ed73a,
    0x49353fea39ba63b12b2bcac1fa28086e,
    0x1dd01aafcd53486a4c1a3a2190e08f26,
    0x1fca8a92fd719f858e718591cf07851e,
    0xfc7c95d827357afab14fadbd3baa703f,
    0x18a6a990c8b35ebd8f1eb8cc7eedd98e,
    0xcccb7005c6b9c28d89ed662f01bcb2fd,
    0x3bdbb92c43b17f26a263fa3b9a325a8f,
    0xaa70b5b4f89695a2bae9f4e14d09637c,
    0xe94c39a54a98307f6be076b52945a007,
    0xb7a0b174cff6f36ed264830e7dc7f906,
    0xd4dba84729af48adc26059b78ed6854f,
    0x2e18bc1ad9704a68148dca9a9b0c8474,
    0x2de0966daf2f8b1c9749c69073bafeb8,
    0xb9c11d5b1e43a07ebba6f4662b0cfd3c,
    0x64972d68dee33360620103f01e5b63f8,
    0x94628d38d0c205844c7820f950a4c583,
    0xdbc0d2b6ab90a559e1262fa8ff1d3269,
    0xd2733c4335c6a72f8f5121c2873029ef,
    0x7e75d99d94a70f4d4fb3edb54d507b36,
    0x6ced1983376fa72bf8594c470632ebb6,
    0x97fcaacbf030bc24b6e876e78ecf5164,
    0x7b77497b32503b12afeb0a5d807150f5,
    0x8547eddfb81ccb94f651bea4c88fcaae,
    0x79999cdff70902cbbfbce123f03177da,
    0xcffe1939438e9b24b6aa0fd22855e81c,
    0x829626e3892d95d7a240adf54d70b24e,
    0x92fae24291f2b3f1732ea6db834bf5a4,
    0x63e22c147b9c3403b47231e07ae8b35f,
    0xc678b6d860284a1c80554c039ab7af15,
    0x5873888850659ae7db2e83297a30b541,
    0x0981dcd296a8736dd58b2a396b5a1669,
    0x9f65789a6509a4402151156aaffdf4b7,
    0x9ff38fed72e9052f65479a629704845e,
    0xe479ee5b9930578ca9bed81039cf1c6d,
    0xe7f28ecd2d49eecdd1a3f98b97eea710,
    0x56c074a581ea17feff78a5d72aa18fe3,
    0x5544f7d774b14aef96d1a9830ba7ffd3,
    0x7b3f0195fc6f290fe19f501dcdf116db,
    0x12153635b2c0cf578e68f253a278535d,
    0x7f5126dbba5e0ca7dc75c481b2cdc0fa,
    0x7a76956c3eafb4137162fa408d9042fd,
    0x3d5774a11d31ab399fadef0bce1a7da1,
    0x8a1b083821f40cb45421bbff426d4e84,
    0x7b4a38e32537df62e7944beeb699c0e7,
    0x950113646d1d6e032ed8b2db03799071,
    0x4da8979a0041e8a9fc4e188df10d5454,
    0x3bc36e078f7515d79de7df26c1457c8f,
    0x5d0a12f27ad310d18fd73517b47f22c8,
    0x7f9d1a2e1ebe1327a3005e22b1bd3e53,
    0xda3a361b1c5157b12bb2c23c899cbc9e,
    0xdcdd7d20903d0c25bfb0766d26526dbf,
    0x36833336d068f707166dbdc38309e26b,
    0xce68341f798933896796f7e11a4c3cba,
    0xab9090168dd05f34562c093e14a87ff2,
    0x43954b3252dc25e53627e637e2413092,
    0xb438c2b67f98e5e9240414702e63067a,
    0x10dcd78e3851a492a6f63494f774323c,
    0xdbc27ab5447822bfc19ea801fb344afb,
    0x9b3cdb65f82ca3829d18e8c21a6c8c61,
    0xb67b7896167b4c8487f9503f9fded941,
    0xbfced1b0048eac503e5098f627c4ed6c,
    0xa9119b60369ffebd6eb063443f58c2ae,
    0x1fff7ac80904bf453427200616f65462,
    0xac12fb171817eee7e3b74fe55c76691f,
    0xaf08da9177dda93dd6316d5008f932ec,
    0x1b0cab936e65c74435a0d4ca7416842f,
    0xb559eb1d04e5e932de4683286c56072d,
    0xc37b45b3f8d6f2ba158754657dc0f21d,
    0xc3a9dc228caac9e96d808b472208eab2,
    0xf3b8b6675a6507ffcb208f7c937f44e6,
    0x9fc477de4ed681dadff1dc38532c3a2e,
    0x67378d8eccef96cb64ee6958558a5d83,
    0x6dd856d94d259236f103b408d1245a6d,
    0xa319ce15b0b4db31a1f19c518c3e0b41,
    0x073973751f12dd5e48f0c2ff42819bfd,
    0x8a8e849eb32781a51472ba925a4c6123,
    0xe1925c71285279f5e3b750660989609a,
    0x74c04bf1790c0efe6dd760ab49ed7373,
    0x4dda48153c94938a67926c593a78bcaa,
    0x9d266d6a1cc0542c5978ff009a18007c,
    0x7440fb816508c4fe5568e6bf328b448e,
    0x13328503df48229f7cc90fbed1165f2b,
    0xd6bf7baee43cac40156f28d728f97cba,
    0x4838d65f6ef6748f2edf603ec74b4900,
    0x1e152328f3318dead48f299033fa9c9a,
    0x8f8419a348f296bf543751766a18d326,
    0x72c8834a5957b51104f25bdd180f31cb,
    0xd7a023a73260b45cc66a3569f903b0dc,
    0x94ebc8abcfb56daece61b42c7eead35c,
    0x9fc10d0f989993e0a705d68144caaf00,
    0xde68a2355b93cae63371ede2968498fb,
    0xa44cfe79ae538bbe1c0c9a220f8fbf8a,
    0x9d1d84fcce3714256631fc26158faebd,
    0x51d2b1ab2ddfb636ef0ec337ff2aef59,
    0x2fd7e4b9e72cd38cc979ef8243e71d7a,
    0x65ca5b96b7552210d6c1a70601c91112,
    0xdd69a0d8ab3b546df3a1da1866141057,
    0x604d51b25fbf70e2d3e2b4c698f2a99e,
    0x73aa8a564fb7ac9e1b4f5d5760ac5121,
    0x1a8c1e992b94114827d0b28f28e7d0ef,
    0xaac40a2703d9bea0f3a2d309d1e72bc0,
    0x764dbeae7fa4f3a61294c73b3f914dda,
    0x1e99b96e70a9be8b56dab15dc8b3fc48,
    0x2c5e9deb57ef47435b77d3202095d45c,
    0x3a938fee32d29981e0d317e26a17ae47,
    0x26e6db8ffdf5adfe5b1d671e17069897,
    0x469356c504ec9f9d937dc438ef99030b,
    0xc8763c5b08d1908cce09ea57087ebea9,
    0x3f6c6af859d80055d0d8fac3d4cfa048,
    0x7f7cc39420a3a54527a5886862d56b94,
    0x9bfb227ebdf4c5ce591673661c00d80b,
    0x89039d79d6fc5c5cd1cc44ae71a9791d,
    0x8fe88b57305e2ab6df5a715ada209d36,
    0xa09e8c8c35ab96de491171944e677dae,
    0xfa7e393983325753d0cf92367e04163c,
    0xd6b6d0ecc617c69967b6a5b051ce8d5c,
    0xdfea21ea9e7557e388d41953044d621e,
    0xb67c1fa481680af8b93bcd9dd369fe49,
    0xca1e3785a9e724e579999db57b235430,
    0x1cfc8bed0d68163920bad52dd13a90d4,
    0xd18d8549d140caea4cccc5ae16a29dd2,
    0x4ed0fe7e9dc91335a0615c69803b77c7,
    0xe4dbf0634473f5d22e2cb0938ba801a0,
    0x1761f93a44d5aefe7ba91914cab50528,
    0x53898e4c3910da552738873108dcba8a,
    0x734de8181f6ec39a4502a97899131230,
    0x2680b122baa28d9738108697800e8bb5,
    0x298af231c85bafaba46ede145d66a90b,
    0x7983eed3740847d5e99d73feedfd98b3,
    0x66c1a2a1a60cd8893267a96bed604a38,
    0x9e17e49642a3e4c1ad66f2c81cc3fc42,
    0xedb454e7badc0805fa2de5ba2d8c3693,
    0x50b704cab602c329d4ca1c86d116bcbd,
    0x4cc317fb9cddd023525f7774ee1dde6b,
    0x66b4835d9eafea22a8e346682e2883b9,
    0x219b97e26ffc81bdae03d84b90df4cb2,
    0x261e4e4c0a333a9dd12735c3d08e24d9,
    0x1fe2cca76517db90b737b467cee71d3a,
    0xd7504dfa8816edbb36970fe334b2a37e,
    0xb9571fa04dc089c8d5c73db7872be26f,
    0x1ddc0325259b27de8aeb0ed56a4177fa,
    0xcf3f4688801eb9aa0199f29dbf7a1802,
    0xf4f5d05c10cab2431caba957a1ff78f0,
    0x38b6525c21a42b0e2abfd2ecbf62492e,
    0x36f60e2ba4fa6800add370c3cd316a3e,
    0xeb3593803173e0ce08a307d218ffbcbf,
    0x9c4cd6257c5a36037bf02813994b261f,
    0xaf0c317d32adaa8a894290366274ef43,
    0x258e5a80c7204c4bc0821c96582294b4,
    0x8b889d624d44885dbd2ce07a63da7db1,
    0xf4d14597e660f85503ba38df61dba3a6,
    0xd4347f66ec8941c34bc8ce1e706bb08d,
    0xe699ed85b0dfb40d7dca263ea9024a3c,
    0x2472f6207c2d04844e876b140c9cda33,
    0xc2a1e7b5b459aeb55bdabc35a2fa1b4e,
    0xab4f6451cc1d45ec383a46ece18c27c4,
    0x63767572ae3d6174532ee826c31a9b81,
    0xa59e0bd101731a2884f3d8faecfd7924,
    0x116d0016cb948f09ba905cc371f066d9,
    0x2cf9c8ca052f6e9f3da882318279b416,
    0x0b090a7560a968e3b3566f6dccae32ed,
    0xabeeddb2dde06ff16eb13ec5ca4cb179,
    0x58efc10b06a2068dab38266303f672c0,
    0xc6e57a78fbd986e0c5e23f8698084689,
    0x2eab8ca63ce802d7502fe8a1ce0a1bc6,
    0x14a195640116f3364664fe5154093ec2,
    0x7c0828dd624ec3903e39d0fefb4ffaf8,
    0xd74bbe77e6116ac74162ffb2b58aed88,
    0x804456af10f5fb535e1e505c7c916883,
    0xebe9ea2adf4321c72185b1f34d275173,
    0x03219a39ee587a30db898d0487271365,
    0x49787fef17af9924ddb7e20f4f0b0a5f,
    0xa1e9300cd8520548e39ecdbb80830b57,
    0x5b45e522e4b1b4efd1e12d09eb3d6c76,
    0xb49c3b3995091a36f33de05912951acf,
    0xd4490ad526f144317ddca4e7cf6a2022,
    0x12a8f216af9418c237dae73934d2b45c,
    0x001f837cc7350524ddf7d6c08847b906,
    0x1877b51e57a764d576a4f4c4bd5b4dc4,
    0xa2853b80f17f58eede916ef3cda04b7a,
    0x993e1de72d36d310f79a5f06f0548dcf,
    0xb3598080ce64a6567980b63a972639bc,
    0x252f59cf0d9f04bbc327c42efcd3dcb0,
    0xd23c8e176d1136008ae99f5a13771265,
    0x1bda0492e7e4586eaeb96c60887bf568,
    0x21e0bd5026c619bf0262cf9cfb1be6f7,
    0x3b097adaf088f94e96f28db3af9f8eaf,
    0x8d14dedb30be846ebd74fc2e2cd130ed,
    0xf95cffa23af5f6f4bfb40b3bf2130455,
    0x3871700761b3f7430d27390428f909cc,
    0xca672b91e9e4fa16e64118d41047bff4,
    0x64c8e531bff53b559f175874ee74dfc0,
    0x241260ed4ad1e87d3c41278759da9b49,
    0x106c09b972d2e822ce243a587f0b6bbd,
    0x7fba195410e5ca3019790b6cab0ef9bc,
    0x7884d9bc6cb569d81b7bb956b20e0ff1,
    0x0647dfedcd894a29fcd55d2a3d92556c,
    0x63573ff03e2247744f6a491571d2a627,
    0x4fc8e9560f91b1232a4bd439b9085684,
    0x1db956e450275779b8eb5b22d497aa9d,
    0xb8d91274b9e9d4fbf6cb137e88679a76,
    0xa2ebee47e2fbfce122bc990578e7dd2c,
    0xd9f1f30ccd97fb0901fc46a5af41ba72,
    0xefed53d75fd64e6b9ee540b9ce7e922a,
    0x2e6d02c36017f67f5a823d7da29de33e,
    0xa9aa4d20db084e9bdcb54247ed750238,
    0xb64be8d8b25396c1bbe346044327e21a,
    0x70cb6af7c2d5bcf03905a0daabfe04e3,
    0x98f076a4f7a2322e1670290ab9118147,
    0xbf84470805e69b5f68163cb777eab10d,
    0x94c3251f06f90cf3ed240589e171a9a1,
    0x3e003e616a6591e92281ad67174bb66b,
    0xb925a6cd0421aff3579ed522cb20efa0,
    0x61bdd1307c66e3009c319f6b6fd2554a,
    0xbf8d5108e27e0d480dd4faaafa80e520,
    0x240ab57a8b888b2074bbae7f87e860f9,
    0xfc87614baf287e07dff318b4d732a759,
    0xef02cdd06ffdb432c371ee9d7d2af132,
    0xa1082c0466df6c0a76cc4be2658dda80,
    0x8215e577001332c8fd9e506d75af7df5,
    0xd39bb9c3a48db6cfc5a0d6e02e703f74,
    0x2738259634305c14631701e42e35f5cf,
    0x61cf4f94c97df93d3055402095f743c0,
    0x1b6baca2ae4e125b37e89e5deff34268,
    0x758f450c88572e0bcc01cd9d386a49cb,
    0x959f587d507a835904a328251bd5828f,
    0xb063e962e045f54dc8dc0ff4362a8c8f,
    0x60e8ed72c0dff5d1ee814f9b3bce7af1,
    0x7b64978555326f9f57ae92f6b7f094ef,
    0xfd080d236da814ba0dbc0ba7bb1c2445,
    0x8c90fd9b083f4558ec57a417b3ae8ad1,
    0x106f72fe81e2c59008d8ec9d3a3a3a85,
    0x7976033a39f7d95218a2e3daa4c6bbe3,
    0xa4ec0132764ca04b68f8161c35388cf9,
    0x733ea705fae4fa77e6e7cc9733d7aa2f,
    0xb4d8f77bc3e56167206c16a493a194f6,
    0x9e21f4f903b33fd9221153dec7e37554,
    0x9d765e419fb69f6dd110651aa3d83db7,
    0xd30c088ba61ea5ef841c207674a2a59d,
    0x5d94337fbfaf7f5bc09b4da25303cd0e,
    0x1a4e4822eb4d7a59e48fec76c3d485e3,
    0x6ffe73e81b637fb34c37f28895f6b9e0,
    0xddf957bc36d8b9ca97fe8d6e8425ea84,
    0x64d0e29eea8838b3e13176251539f9db,
    0x08dd9bdfd96b9f634e887953d43713f5,
    0x087e79e5a57d1d1332dc49e0f8b96e5c,
    0xe328e230e3e2b3fb1240f0315707df84,
    0x1c2559e30f0946bee22f60c878f03163,
    0x720bf5f26f4d2eaa733ede4131a1662d,
    0xb0774d261cc609db5c82e1f58657d112,
    0x443f64ec5a3711956b39b17300529a95,
    0x4112cf68649a260e9a366e73434c37a3,
    0xd813f2fab7f5c5ca3af04822ac63ca77,
    0x660d3257380841ee6c20030dcbb3e153,
    0x59ac2c7873f910a3812bc00a41d9de43,
    0xe846963877671a179d698757f729c30b,
    0x93b633abfa3469f8764407e6839c724e,
    0xc0c0f5a60ef4cdcf347b8bcf7aa8a816,
    0xcaf21ecd4377b28ca15cbc1e73577012,
    0x57277707199b81758387a4b08cc28a2e,
    0x506c11b9d90e8b1dce827a97fe830269,
    0xd83cc2687a19255ffc3677ad21589c7a,
    0x4a29c6465a314cd1c902d33e0a464ec1,
    0xed2df21216235097fb39294699a1e2aa,
    0xb5635c95ff7296e2d7bf168e258f8f01,
    0x22af003ab672e8112e602229d2b37a22,
    0x52e762596bf68235821bc37e2c1f8912,
    0x9aeba33ac6ecc6b0c57a1ba0f260acc5,
    0x944f6de09134dfb6ce78cb667346e38c,
    0x6c47bec883a7de39bb39eae7a290fcb0,
    0x6ad047c430a121041bbc42a45ca20618,
    0xa5b1cfdba0ab4067ef37e286f590dcfb,
    0x7c45d833aff07862d9708ed7ec641deb,
    0x5092ef950a16da0b12c5e2f484724605,
    0x9338e69c052b8e7bf3e85d7b8a77b033,
    0x455a4b4cfe30e3f58ee17c0021438904,
    0x6b02e63195ad0cf8462f7fba7c2868f2,
    0x6b17b224bad6bf27fb7a14d6137fa5b1,
    0xd1e0ccd25bb9c16999ceec99465fe08c,
    0xde0c89a556b9ae70a4be0f0017879351,
    0x50065e535a213cf648c9c74efaeff5a7,
    0x9c1169fa2777b874682f784ba23dc02e,
    0x78edefd694af1eed226e96a540113353,
    0x6dc93d9526a50e68aa04b725850e2e32,
    0xee97f453f06791edbc3338f53303ff19,
    0x32ab0edb696703d394d80fc8e2559d54,
    0x3a6853c7e70757a7b470b50e6769b2e4,
    0x31865ced6120f37d9d5caf79d12f737c,
    0x67fef95d926078900335ce6790cce15a,
    0x1f2b1d1f15f6dc9ca9d88c05289cbd6d,
    0xb69e38a8965c6b65f3d1d29b153dbd5e,
    0xaa9119ff184cccf44104764512849057,
    0xf43c732873f24c131d6d02af20051f53,
    0xfb4a3d794a9a80d2c8e487152b256129,
    0x3550c2321fd6109caf60d64fbda8f2ce,
    0x371f77e76bb8417ebf6d091a05417402,
    0x6bfa9aae5ec05779da184d8ff54f3fc1,
    0xcd04f3ff001a4778b83657ca6633e2ac,
    0xe3273522064480ca75d3e0b8feedbdb7,
    0x9f91508bffcfc14a26a58648fd56deab,
    0x049a7f41061a9e605da2dbf10323e25c,
    0xfcb6be43a9f2fe9b1e3ed1a390724f66,
    0x08de8a1c7797da9bc76159619f3af003,
    0x8f9887e6078735a1b4af22b5f5b26389,
    0xb5b4071dbfc73a66466792d7a2e6c6fa,
    0x230e343dfba08d3366feaacd911b81a2,
    0x43ed7f5a0fae657d94c1e6508f5f1f47,
    0x3a88a0fbbcb05c634206c6c80ef8fd9f,
    0x21874b8b4d2dbc4f3c1dea6d0afcae52,
    0x1bdea12e35f6a8c9e79cf8044680e415,
    0x53c065c6c8e6352848676fcc85844eb5,
    0xe34a1d250e7a8d6b62f20ecb301144ad,
    0xd6b04d3b7651dd7e2778709cb5ff3fc7,
    0x5e90277e7cb39e2dd87c68010650c250,
    0x2c046f22062dc67dbe61c9fc8adc7260,
    0xb10bb459132d0a26eaec3f6695236d35,
    0x3fa9ddfb67e2f199d47e91679b5bbefc,
    0x0e09b88e1914f7af195693617db7534c,
    0x10e8b35af3eeab372725134b52fd2c81,
    0x9eedeca8e272b93322770ffa079a1704,
    0xd4c718bc4ae8ae5f012c2a69dda5ad22,
    0x81536d601170fc2009338c04f427a66f,
    0x91b534f885818a060d85fd7519aa9dec,
    0xec8177f83f9009780246a0f9fd642861,
    0x190e714fada5156eac615f38f5a451ea,
    0xb592bf39b036496307a44f6430336f1c,
    0x89c350c893ae7dc12184303422b53201,
    0xac042e70f8b383f26e47c77585ab8164,
    0xb49b52e587a1ee6055d75131c650586c,
    0xfb152fe3ff26da89420514ac928637fa,
    0x3e666e6f69ae2c15bb00290a9289ce13,
    0x3b544ebe544c19f9321f8022ccc2553a,
    0xe805a1e290cf24567effcb24d14c9d18,
    0x24b33c9d7ed25117dc418c09511a5174,
    0xe74733427b72f0c11130c8b2334d05c7,
    0x0a804d18b709747557f10554d8e9323b,
    0x57e3306d881edb4f90a5ce5a89ea0b56,
    0x4ae7d6a36eb5dbcbc27327f936e68d1b,
    0x2d8d5432157064c8417b730cb2a966b0,
    0xd1e649de1e7f268b1d301ea15b8ea672,
    0x8a328a1cedfe552cf14dee3399ddf91c,
    0x07a3aec79624c7dabd989097807f7fbf,
    0x84547ddc3e203c94c3a0533a6d96954b,
    0x990a98fd5071d2631992575160c43696,
    0x1a4ff12616eefc89d49493b523ad7777,
    0xf6f7fd1431714200b77c0f9cf9e436f2,
    0x30c05b1ba332f41ce62702fef78982ef,
    0x8d2636b81555a786236a476ed9466eb7,
    0x46c9feb55d120902e304cfd61b4f416c,
    0xccec0a73b49c99212cb2ea092e5f1215,
    0x4e9d2827355fc492e2004d8b7660e169,
    0x19ebb029435dcb0f7fcabe79c4442ade,
    0x4659d2b743848a2c3ec3d0686df24ad5,
    0x963ef2c96b33be318f3d9e86c7b31fff,
    0x74f85198b05a2e7d6ba130e1edb0873d,
    0x5a0f544dd2b1fb1840fe52eaaac04a87,
    0x03727073c2e134b1789eb8c74c29b5bd,
    0xc7f6aa2de59aea614630d70199d8fe85,
    0x352787baa0d7c22fc4f6e06e6eadb36d,
    0x9853eab63b5e0b352d9cd0536bddc355,
    0xabbdcdd7ed5c08609522ac8d318de072,
    0xcf05daf5ac8d77b0dac15872053fb2a8,
    0x49cad48cebf4a71e4b63b05120422ba5,
    0x7a4c10ec2158c4a6c4f797ac06e0c775,
    0xd9e92aa246bf719e4b9efbbed8c2bd98,
    0x13ae978d09fe555767b2d7640bd6d12a,
    0x730499af921549ff69216ca21f6e1bf4,
    0x4e4b705b92903ba449ab0589118fe345,
    0xff577222c14f0a3a90b672aaf0764986,
    0x55b6344cf97aafaec24aa6db9b0e9300,
    0xb862225b055b69608478cc06efce550e,
    0xcac09afbddd2cdb482e3ec2ccd28350f,
    0xdaf8e9829fe96b5fbc059bb74b993690,
    0xb5fdfc5d3132c498e31111d14445238b,
    0x310cb380db6f7503a7ed1df77e79a736,
    0xe87fbb46217a360ea137dc0582888c63,
    0x2102ae466ebb11487630e70040281934,
    0xf8549e1a3aa5e00d2c17c3b74634a6d6,
    0x07a69afdcc42261a05eff03a838a4fb4,
    0xc4c118bfe78feaae3c28d21d40d4f80e,
    0xf9f4892ed96bd438beb9cec18b163f7c,
    0x1af3dbe25d8f45dadc75195297484115,
    0xf5b4b0b0d2deeeb4548955be3bde572b,
    0x962aceefa82e1c84c8281bde4d280b81,
    0x046e3ecaaf453ce9914da129a132922b,
    0xf05d129681949a4cff11d08f1cee77a4,
    0x964781ce734b3c848070295bd01f6bfd,
    0x9c2ed44081ce5fbd006a9346f317a9c9,
    0x522e23f3925e319e37e62ccdf9739fc3,
    0x177e00f9fc32f7915cbf8b753a7e703c,
    0x2bc60a63a6f3b3f2fe0f162fcebe01c8,
    0x222bbfae61725606094f481a19d464ff,
    0x486289ddcc3d67809737e370b3c679cf,
    0x7dc7785b8efdfc8060d67575f3b9b1c2,
    0x8af38731c02ba9803948fbaf41196093,
    0x1fab64ea29a2ddf70c204d1cfdbf6e2a,
    0xe4d9429322cd065ab7d2d42a9be29c2a,
    0x9da058c67844f20c49fae729fc2974b3,
    0x24c0e332b70019b06fb26356dad98ed6,
    0x233003b5a6cfe6ad4847b6cc14eeffd4,
    0xd586bd01c5c217f63b5285a9f0152c99,
    0x5e5637885f29bc2bff1fe7f4b91cff4c,
    0x7eba726d8c94094b16e20774363e99d0,
    0x0a56a5f0bfe39272437d1aa9cb4159e0,
    0xd79476a84ee20d067bdcc9d5c1c4da0b,
    0x9e4c1269baa4bf378fd087733782eecd,
    0x17efee45b0dee640bf94dee3ad478cda,
    0x1d95b0a5fcf90bc683c94bbe4c623bf5,
    0x93cbe0b699c2585d08fda03aa45ee9ba,
    0x65fa4f227a2b6d79c188f92d4d403856,
    0xd5f9e858292504d5fd6611dfb12345fc,
    0xc2b5a03f71471a6fef003ffd18aecc12,
    0x59300222b4561e00b7b474cbf2934019,
    0xce2f8642ca0712dcbc2a55e58b30deee,
    0x7ca9723fbb2e89883e77de18337cda42,
    0x2785338347f2ba082f81e058ffb75885,
    0xc61bb3a141e50e8c8f0c300cf585707e,
    0x150f361dab9dec26cf4f4c536e0e2af2,
    0x9f6a419d382595f424c81ee5fd39a8e4,
    0x64a53dc924fe7ac92841577e66ad726e,
    0x142de49fff7a7c3d68090c81a1357214,
    0x0c335248857fa9e7a6aa70d44a613a24,
    0x0a9c32d5eae45305db805d26087f4db9,
    0xe6c42178c4bbb92e6dd954b45a122182,
    0x71f1ce2490d20b07c34729fd9f1948a3,
    0xf1bcc3d275afe51af0682ca0764cc153,
    0xe728e8c83c334074bf29824279ca73e1,
    0x96fbf83a12884624f0c70fb4e725caff,
    0x81a1549fd6573da51e18cad809e9eedc,
    0x5fa7867caf35e149a893a8fa258f383e,
    0x56986e2ef3ed091be78c30cc1179b849,
    0x917f1dd5f8886c612e4432e6ce4996d9,
    0xd20d8c88c8ffe65f576ec7a84e0b932d,
];

const WHITE_TURN_MASK: u128 = 0xf8d626aaaf2785093815e537b6222c85;

const CASTLING_RIGHT_MASKS: [u128; 2 * 2] = [
    0x31d71dce64b2c310ca3c7f8d050c44ba,
    0xf165b587df8981908f50a115834e5414,
    0xa57e6339dd2cf3a077568e6e61516b92,
    0x1ef6e6dbb1961ec9d153e6cf8d1984ea,
];

const EN_PASSANT_FILE_MASKS: [u128; 8] = [
    0x70cc73d90bc26e2413099942ab633504,
    0xe21a6b35df0c3ad7946c73529a2f3850,
    0x003a93d8b28069623d1adc27d706b921,
    0x1c99ded33cb890a1994b8bd260c3fad2,
    0xcf3145de0add4289f4cf0c83cace7fe4,
    0xd0e4427a5514fb7254807a18b6952e27,
    0x77c621cc9fb3a483e2a1aff40d08315c,
    0x67a34dac4356550b47ec43ffbc092584,
];

const REMAINING_CHECKS_MASKS: [u128; 3 * 2] = [
    0x1d6dc0ee61ce803e6a2ad922a69a13e9,
    0xc6284b653d38e96a49b572c7942027d5,
    0x803f5fb0d2f97fae08c2e9271dc91e69,
    0xb183ccc9e73df9ed088dfad983bb7913,
    0xfdeef11602d6b44390a852cacfc0adeb,
    0x1b0ce4198b3801a6c8ce065f15fe38f5,
];

const PROMOTED_MASKS: [u128; 64] = [
    0x2f9900cc2b7a19ca2b9178eb57f3db25,
    0xf75235beb01886d317d16678351d3778,
    0x8ae7e29889ac996488b17afbdae836b1,
    0xad30091ce7cb4204a8985bb047c388b1,
    0xaae118773ddd4e4d2577b875de120fd2,
    0x8ec514ce4736aa07bdf2fdea13ee21fd,
    0x26a412bd8cef4f153bcf1cf1605da5e7,
    0x1bdce26bd9af059f6ea6f4fb2ca4d856,
    0xea5f4ade5acc0516b1198a9621b237f4,
    0x69ab7ebc076505656c87686b28782362,
    0x3e655f895a188a1c5158703d9536de86,
    0xf394f6882a114d65b3126fafbea75501,
    0x3173cfa2be5bd4d380d3335852547580,
    0x434d20d2ca00ae71c5afc4f44d5ab019,
    0x3ba297f73d338c937aff7035ddcde586,
    0x099ba1b0205a5ea5c338837c953120b2,
    0xc49f050b5e1c56537f8b4b715fc6eee8,
    0xe14eec50a9c690e876d1cd962b7c0005,
    0x2571cc79f4ce0169ad3db13d420b8915,
    0xde0f98d6002f4323e5a6076284061351,
    0x0682220b02e5c3e87f61ddc8b19de688,
    0xcb900d3a6b38c39d8648ca9be5696f33,
    0x24620fbf09d50d66469dc25e1b32d323,
    0x0f40a9b2781a119d4f529fa289578425,
    0x83c6980df0d0493282d550f6a40a3d66,
    0xab6f9af720cb5df48e791844cfb87476,
    0x1c906974166ee8d4b229ab8bb6054dad,
    0x9c1ba3db0784ebda50a0b7e3c796ad7e,
    0x81a19098d16aa929186c0c4a7a5d68d1,
    0xfce56173c63ccefd748301e9e571a670,
    0x43cb7aa20c6209c22119955cd577ee40,
    0x7e96e2ae86924bab6c57c1380ffb21fb,
    0x01860725034b0fefadd607ff5aaaf995,
    0xf74d369066ec4e96d41ec439c73a3e0f,
    0x1ae9962c6e0d12323ee343c1d9837a9e,
    0x5d66fa465ccfc56097707414bf743321,
    0xe9c13ae1fc36afaae6a8ec453ed67917,
    0xcaec4035fb840be403845f0849e183df,
    0x839d28adafad0f8fccd5f9e7ce7e601f,
    0xe4703b6e30422003e2c04ba3484232d8,
    0x1e2fd5b2827d5e435d9b925a3e6af022,
    0x96f1e8d8b94bd960a852b77063b9e148,
    0x90f2075c3f43960c279d2c0c53ecaac6,
    0xc48e0774c4f9134f39f0827a4f811c72,
    0xf17e5f6a2cb000c73b064b8614517b48,
    0x6248409bf55a4925db19bcf000dd394a,
    0x967bd94eb30505cca63dddb617c59634,
    0xe91e89853f9e844fece83ffa46fd173a,
    0xb841038e24193f0801c4d4d39c11557f,
    0x46f3b25cae82a6cca618dd21a5f6b67f,
    0x3e97e042449e3ed58356bdf440563fb0,
    0x868a166af46dcbd2f35ad55d727351a1,
    0xf71be788b3fd1a7aa3a0c83ec173d41c,
    0xcb6d65410533cc37a8b2fe93fc5ddd19,
    0x7e30d70559efaedc984fd612fa3936bd,
    0x32db0f5ca18159ce71c7e142bb029d2a,
    0x97a9116e874228c5759f8a9460c634a6,
    0x85ee68ee3a1752976fa2ca2ca3bdeb90,
    0x076a14170b409e2aea5f0228fc314b97,
    0xbad49d47dc95855bc40c265d97e763c0,
    0x636187d94ded991e201d68d0d89f0e31,
    0x962e50971f09cfab1b769456c77879cf,
    0x8f16c910d6776589a8baaf8b31da09a5,
    0x7e3de4bfbef5566f06a79ac414c9632e,
];

const POCKET_MASKS: [u128; 5 * 2 * 15] = [
    0xb262e9f9d61233206e21a47d5b561a1d,
    0x91533947cdaa8bec4263a757e414fe44,
    0xa13b56b45723a3d493c43f67cf55b53f,
    0x9a35cce29ca3ac75a89732f339d35eec,
    0x2716940e1d4f28d75df4ac25c29fbebf,
    0x7447209cfb79306662059230cedcd78f,
    0x5cf91d8ae6402e1a9b0f7261932d2c8e,
    0x4625588d38487ac5fc0f99f00e0cc0e7,
    0xe42ec6191353e3bdcba8a5a02c351aa5,
    0x478e6cc8f6b2dada77a6c01bd0174d69,
    0x1726fc948b994b87c200e5264100e463,
    0xfb9d2e5a66b467413f340a89f525effe,
    0x7f668e401ffe9e6ff748b2be597b2f7a,
    0xee4d6fe11c46a236f7b2eddf7b8838c2,
    0x006cb70064259959bdec7e9f4317a678,
    0x33535a7c4def1b245e022239fdf20b36,
    0x479e792f8171fc29fdd92fa05b03b629,
    0x656a6e71de97097580cdab95a89927dc,
    0xcada3e48618a1c2b92cb516b8eba4a30,
    0xb37ad7262db9c99eac950f8bce3af2d9,
    0x85ae25402a311d5d9a43060d3aaae01a,
    0x3de4e82d52dbb44cf12b2f6012f9333c,
    0xb1c8499674464c218ae9143ece61584a,
    0xf1c1853cc6827b84676f70930e72993c,
    0x51f97ed3ba004fb025e0f5f014476a1f,
    0x00da9ede878e3e981e33827f042de978,
    0x3cd0fd658e1cdb12dd158e4a7838524d,
    0xac2940b688a1d0f92b75316dfa1b15e2,
    0xe51acb5b336db0df0283b57f325ea495,
    0xcf7517fbdcb16174146e60f56ab91765,
    0xdfe901aba4a2ced3f50d2497f8b12819,
    0x24bfd4b72c8852eb7600c53f3c60308b,
    0xf085bcd9711883d4b75208a6056dc7e9,
    0x41b71908a3d86274e5f0eb83c20e921b,
    0x6d604cc0a2df1a697529d0a0c95f08ed,
    0xaedf8291e0048c3933c7b70ee04a511c,
    0x09d3c83f59547935727a256dad06cc11,
    0x257d5c7ebc7182421b26058e1ba73008,
    0x56ac1c998f5c2ede09c48fc7e167f3b0,
    0xa25c0b0679937316954f57cdf6076cd4,
    0xa9a2a7e200faa93632facc37b50d925e,
    0xb8e7ca4716cf9d49259698168b89f941,
    0x9b253f89247c4c1db4479bfb3575d3fc,
    0x1e701e2a73f9dc4be69d20380ad45ef5,
    0xcdf351b289aa5a840ea72e455dfb08e6,
    0x2e4e118fc45fdc0d7f834fab71613f89,
    0x80247d70885ad5ce0bef7b04290fa4d3,
    0x0a99dccfce316ca0425d9a9261abb5b9,
    0xb5553435dae76840231bfd9dfb70f61e,
    0xee562004d5d14158f2ce79a69837967f,
    0x551b5fa3ec7166a2f9012eb6947f5b8c,
    0x2dbb493c6e9fec06e867f9c703503bf1,
    0xf06b4c65f4bb14a1ee98c9010d1d3cbc,
    0x5f0b44d98013acb995fc0aa1222389e8,
    0xce7dbafa734bba8abf7533ebb6c99102,
    0xe009c0e355a77913bc4153720b7d8489,
    0x21918f473cb6decfe892965f4753afa0,
    0xdcf11e80dc14763feaec3e0774781d2e,
    0x7ac21357500fb0c600b698a9c3390404,
    0x28abe0a3761e326c84c94d9a3d13f9b5,
    0x30b8e3da17d34c6ee4a910d516e80a37,
    0xd999d38ffa5d771e30a99425bba73df4,
    0x8a7e0d1367d70b281fe92363cf099b7e,
    0x9157bfe7ac0717963fce173a5e427cb2,
    0xadda94b21edd779a3cf044d0bd7bfb26,
    0x6f555cf7856f0d6314dd9fdfd382638a,
    0x5b2a5b2788adc947a4ec5d64116068be,
    0x500c782c8c562a42970d0225c3155df4,
    0x20f8b3f7059d888496bb58faf0fd1692,
    0x79c890ed3e95f3f4183f9136b8160b83,
    0xe64dbd474ddcf8ca85c179d22cce92b9,
    0xa94966fbf7f270d5d355d7752b4405ff,
    0x2473b4e6ad9faa9ada7b7aa372230d64,
    0x98abdf9fa4b487e68218ddc4550f6260,
    0x75fa1ecb0717029a090f2692c727e1a1,
    0xf6053757646a08bace74c18e0f75f7f8,
    0x060e2788d99813aa075081d9944cf832,
    0x5fa61c63681ebbc890fb343362ef172d,
    0x90bbf42db708006aac07cd32cdd4a6c5,
    0xb525460ec1c159163684e6c1eaf9e5f5,
    0x2696070a4502024d0f515afc1356a5ca,
    0x158087442731df68282bbc7b2209e436,
    0x65010c3ea0acfdcfc0172402afbbcbc5,
    0xb28ecdf305a7a83134e345bfa3c47ceb,
    0xfd037a2e2a2e54e8a56824690a26a07d,
    0x5f09f3763f6a488282de3ff226f8520b,
    0xe0125e53c4e64b83d6e502a609ba9dde,
    0x1de44a244be3752a3b40c681d5b6e330,
    0xf78919dfb05f031c23c2312838f00cc0,
    0xbf81caebad91d8e16e63a230b2eab193,
    0xbc3780dce0bd58d59a08aa69fb121fc8,
    0x65b5fb1afa5c5714551be806bb80e780,
    0xb7ddb798d0c1ff235ab7fd28c7b96a6b,
    0xa823d99d1504f4d6b3277b903da3eab9,
    0xa3c526e07f1cf98daf84fcbb2d16fa25,
    0xa848f93e7a83ece41c53b33de31ba544,
    0x21e3941600abaaec4186f24fae7b9c26,
    0x534a070449a9238d4bccd3e248e2a69c,
    0xf86c2e4bb82d39238d6b46aee9ae0606,
    0xa594b44c256b41f87b281261c9e1028e,
    0xf1503a710531b6776c501072a1108abe,
    0x171a27a1b9911e11a8c4da9ba7c8a22a,
    0xd8d8a26ac022ebe688d5489e8bc29294,
    0x151ca9f641352f331381a967122b289a,
    0x553b499dc1eae685dc944a008e97acd7,
    0x0137684bed65e27e63838f936ef425de,
    0x254a12bd9efa353539bd05197acaddb5,
    0xf8361f0b0a35ef6df5871ddb63b0aecf,
    0xa7b7e76b7ff82166cd9a19a4addac30e,
    0xe266e0067bc7f39608c6dce88d2bfe12,
    0xbdd8a9037f5d0298ea8c0b4c4097cc43,
    0x2d5977c3f88a2a3174fb47cb2de1371f,
    0x4587cd651a3bb45f8c2406a60633f3a5,
    0xbcdc3c56ad971eb0c40387658bd2d9bf,
    0x248b2073706e18444616dae41e7f6769,
    0xab03444dfb15bd0aff41e4fd3d1a34ee,
    0xbcaff3134756ab78d9c4e712cc561f6a,
    0xeea844cf3e1db285393520a31d54572c,
    0xb917fdb80f3551161cc02ca62684138e,
    0x21931f559ecefa34c1b9f65a9989c1d5,
    0x7170c6436114a4c286e3a7966174637c,
    0x73d2a7c1017a2aaf80d8247d1168300a,
    0x3b855d1755dce20e1996e2ac2b938629,
    0x37e35078817f0dbdb26f006263639c6a,
    0xe59b1e3389a1aad3fe1f7c18126abdd7,
    0xbad11ebe3c3df23983b9090f7e58e659,
    0xd54aad8a64c65c2728253df5164c46a3,
    0xeadb37a4f7fbeb4c3448dc022a87a231,
    0x5453586c4984a81c72ebd4eb6f76301d,
    0xb6777cd5e1b16bcc1f5cf5c027f9df47,
    0x24b690161baa0d65bcda2313c8ee1152,
    0x5bd3613d2ee222fdb2ebc33394603af6,
    0xc928bda035f8c39d84b3b1b6fa01cb1a,
    0x29e8eeca9b09c735e16b42d53a9ecf6e,
    0xdc35bba3f78ed4ee27b24383307c7a88,
    0x1753c5b3ef820c8101635963de0d35f7,
    0xef3368ab56565ae7d5667fb942e52b2f,
    0xebf48bd35c4ead40958faa7325a97d06,
    0x529b39d015e4975570433ecb73b7ad92,
    0x697ea70620e987514ab8a46b4c36eaaa,
    0xebcabe3d4cbc02121869111d7c4fe1a8,
    0x2f424e669d2a7f1bfcd46428ff1e2a1e,
    0x6ae47a9c22302f58b65e4b536bfa3559,
    0x83f9a7b574523121180b7095853d37f3,
    0x77b6860daa7a39d5bb0226e8c1543063,
    0x1611e306f167e512708d753a0092df11,
    0x2d78f39e1adbaa9d9992315a0c7adfe5,
    0x1aada836dedb3ba7e12508294de8e35e,
    0xf37991753c7df55849960c597d119ace,
    0xe80840e623a19d0857b7ccbb21f74d1c,
];

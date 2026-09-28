/-!
The CRC-32 of APPNOTE §4.4.7, as an executable reference: the reflected
polynomial `0xEDB88320`, register preset to all ones, and the final value
complemented. Bytes and the register are natural numbers.

`update_append` proves that the checksum of a concatenation can be computed
incrementally, which is how a streaming reader computes it; `update_lt`
proves the register never leaves 32 bits; `check` is the standard check
value of the ASCII string `123456789`.
-/

namespace Ledger.Reference.Crc32

def polynomial : Nat := 0xEDB88320

/-- One bit of the reflected shift register. -/
def bitStep (c : Nat) : Nat := if c % 2 = 1 then (c / 2) ^^^ polynomial else c / 2

/-- Feeds one byte: exclusive-or it into the low byte, then shift eight times. -/
def byteStep (c : Nat) (b : Nat) : Nat := Nat.repeat bitStep 8 (c ^^^ b)

/-- Feeds a byte sequence into the register `c`. -/
def update (c : Nat) (bs : List Nat) : Nat := bs.foldl byteStep c

def mask : Nat := 0xFFFFFFFF

/-- The CRC-32 of a byte sequence. -/
def crc32 (bs : List Nat) : Nat := update mask bs ^^^ mask

theorem update_append (c : Nat) (xs ys : List Nat) :
    update c (xs ++ ys) = update (update c xs) ys := by
  simp [update, List.foldl_append]

theorem polynomial_lt : polynomial < 2 ^ 32 := by decide

theorem bitStep_lt (c : Nat) (h : c < 2 ^ 32) : bitStep c < 2 ^ 32 := by
  unfold bitStep
  have half : c / 2 < 2 ^ 32 := by omega
  split
  · exact Nat.xor_lt_two_pow half polynomial_lt
  · exact half

theorem repeat_bitStep_lt (n c : Nat) (h : c < 2 ^ 32) : Nat.repeat bitStep n c < 2 ^ 32 := by
  induction n with
  | zero => exact h
  | succ n ih => exact bitStep_lt _ ih

theorem byteStep_lt (c b : Nat) (hc : c < 2 ^ 32) (hb : b < 256) : byteStep c b < 2 ^ 32 := by
  apply repeat_bitStep_lt
  exact Nat.xor_lt_two_pow hc (by omega)

theorem update_lt (bs : List Nat) (c : Nat) (hc : c < 2 ^ 32) (hb : ∀ b ∈ bs, b < 256) :
    update c bs < 2 ^ 32 := by
  induction bs generalizing c with
  | nil => exact hc
  | cons b rest ih =>
    simp only [update, List.foldl_cons]
    exact ih _ (byteStep_lt c b hc (hb b (by simp))) (fun x hx => hb x (by simp [hx]))

theorem check : crc32 [49, 50, 51, 52, 53, 54, 55, 56, 57] = 0xCBF43926 := by decide +kernel

end Ledger.Reference.Crc32

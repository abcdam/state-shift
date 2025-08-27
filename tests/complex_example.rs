// #[rustfmt::skip]
use state_shift::{impl_state, type_state};

#[derive(Debug)]
struct Player {
  race:        Race,
  level:       u8,
  skill_slots: u8,
  spell_slots: u8,
}

#[derive(Debug, PartialEq)]
enum Race {
  #[allow(unused)]
  Orc,
  Human,
}

#[type_state(states = (Initial, RaceSet, LevelSet, SkillSlotsSet, SpellSlotsSet), slots = (Initial, Initial, Initial))]
struct PlayerBuilder {
  race:        Option<Race>,
  level:       Option<u8>,
  skill_slots: Option<u8>,
  spell_slots: Option<u8>,
}

#[impl_state]
impl PlayerBuilder {
  #[require(Initial, Initial, Initial)] // require the default state for the constructor
  fn new() -> PlayerBuilder {
    PlayerBuilder {
      race:        None,
      level:       None,
      skill_slots: None,
      spell_slots: None,
    }
  }

  #[require(Initial, B, C)] // can be called only at `Initial` state.
  #[switch_to(RaceSet, B, C)] // Transitions to `RaceSet` state
  fn set_race(
    self,
    race: Race,
  ) -> PlayerBuilder {
    PlayerBuilder {
      race:        Some(race),
      level:       self.level,
      skill_slots: self.skill_slots,
      spell_slots: self.spell_slots,
    }
  }

  #[auto_assign(level = some_level_calc)]
  #[require(RaceSet, B, C)]
  #[switch_to(RaceSet, LevelSet, C)]
  fn set_level(
    self,
    level_modifier: u8,
  ) -> PlayerBuilder {
    let some_level_calc = match self.race {
      Some(Race::Orc) => level_modifier + 2,
      Some(Race::Human) => level_modifier,
      None => unreachable!("type safety ensures that `race` is initialized"),
    };
  }

  #[auto_assign(skill_slots=skill_slots,race=race)]
  #[require(RaceSet, B, C)]
  #[switch_to(RaceSet, B, SkillSlotsSet)]
  fn set_skill_slots(
    self,
    skill_slot_modifier: u8,
  ) -> PlayerBuilder {
    let skill_slots = match self.race {
      Some(Race::Orc) => skill_slot_modifier,
      Some(Race::Human) => skill_slot_modifier + 1, /* Human's have +1 skill slot advantage */
      None => {
        unreachable!("type safety ensures that `race` should be initialized")
      },
    };
    let race = Race::Human;
  }

  #[require(A, LevelSet, SkillSlotsSet)]
  #[switch_to(SpellSlotsSet, LevelSet, SkillSlotsSet)]
  fn set_spells(
    self,
    spell_slot_modifier: u8,
  ) -> PlayerBuilder {
    let level = self
      .level
      .expect("type safety ensures that `level` is initialized");
    let skill_slots = self
      .skill_slots
      .as_ref()
      .expect("type safety ensures that `skill_slots` is initialized");

    let spell_slots = level / 10 + skill_slots + spell_slot_modifier;

    PlayerBuilder {
      race:        self.race,
      level:       self.level,
      skill_slots: self.skill_slots,
      spell_slots: Some(spell_slots),
    }
  }

  /// doesn't require any state, so this is available at any state
  #[require(A, B, C)]
  fn say_hi(self) -> Self {
    println!("Hi!");

    self
  }

  #[require(SpellSlotsSet, B, C)]
  fn build(self) -> Player {
    Player {
      race:        self.race.expect("type safety ensures this is set"),
      level:       self.level.expect("type safety ensures this is set"),
      skill_slots: self.skill_slots.expect("type safety ensures this is set"),
      spell_slots: self.spell_slots.expect("type safety ensures this is set"),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn complex_player_creation_works() {
    let player = PlayerBuilder::new()
      .set_race(Race::Human)
      .set_level(10)
      .set_skill_slots(10)
      .set_spells(10)
      .say_hi()
      .build();
    assert_eq!(player.race, Race::Human);
    assert_eq!(player.level, 10);
    assert_eq!(player.skill_slots, 11);
    assert_eq!(player.spell_slots, 22);
  }
}

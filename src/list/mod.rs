use crate::algorithms::{chabad_holidays, israeli_holidays, shabbos_mevarchim};

use crate::algorithms::candle_lighting::City;
use crate::args::types::{
    AppError, CustomHoliday, Daf, DailyStudy, DailyStudyOutput, DayVal, Event, Language, ListArgs,
    MainArgs, MinorHoliday, Name, OutputType, RambamChapter, RambamChapters, RambamThreeChapter,
    YearType, YerushalmiYomi,
};
use crate::prelude::constants::{get_minor_holidays, GEMARAS_FIRST_CYCLE, GEMARAS_SECOND_CYCLE};
use crate::prelude::get_omer::get_omer;
use crate::prelude::print;
use crate::Runnable;
use chrono::prelude::*;
use chrono::Duration;
use heca_lib::prelude::Chol::NineAv;
use heca_lib::prelude::*;
use heca_lib::{HebrewDate, HebrewYear};
use rayon::prelude::*;
use serde::Serialize;
use std::convert::{TryFrom, TryInto};
use std::io::stdout;
use std::io::BufWriter;
use std::io::Write;

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Return {
    list: Vec<DayVal>,
}

impl Return {
    fn pretty_print(&self, args: &MainArgs) -> Result<(), AppError> {
        let stdout = stdout();
        let mut lock = BufWriter::with_capacity(1024 * 1024, stdout.lock());
        self.list.iter().for_each(|d| {
            let ret = d.day;
            let year = ret.year();
            let month = ret.month();
            let day = ret.day();
            let name = d.name.clone();

            let mut year_arr = [b'\0'; 16];
            let mut month_arr = [b'\0'; 2];
            let mut day_arr = [b'\0'; 2];
            let count_y = itoa::write(&mut year_arr[..], year).unwrap();
            let count_m = itoa::write(&mut month_arr[..], month).unwrap();
            let count_d = itoa::write(&mut day_arr[..], day).unwrap();
            match args.language {
                Language::English => lock.write(b"Night of ").unwrap(),
                Language::Hebrew => lock.write("ליל ".as_bytes()).unwrap(),
            };
            lock.write(&year_arr[..count_y as usize]).unwrap();
            lock.write(b"/").unwrap();
            lock.write(&month_arr[..count_m as usize]).unwrap();
            lock.write(b"/").unwrap();
            lock.write(&day_arr[..count_d as usize]).unwrap();
            lock.write(b": ").unwrap();
            match name {
                Name::TorahReading(name) => {
                    let mut res = lock
                        .write(print::torah_reading(name, args.language).as_bytes())
                        .unwrap();
                    if let Some(l) = d.candle_lighting {
                        res += match args.language {
                            Language::English => lock.write(b". Candle lighting").unwrap(),
                            Language::Hebrew => lock.write(". הדלקת נרות".as_bytes()).unwrap(),
                        };
                        if let Some(candle_lighting_time) = l {
                            let mut hour_arr = [b'\0'; 2];
                            let mut minute_arr = [b'\0'; 2];
                            res += lock.write(b" ").unwrap();
                            itoa::write(&mut hour_arr[..], candle_lighting_time.hour()).unwrap();
                            let minute_write =
                                itoa::write(&mut minute_arr[..], candle_lighting_time.minute())
                                    .unwrap();
                            res += lock.write(&hour_arr).unwrap();
                            res += lock.write(b":").unwrap();
                            if minute_write == 1 {
                                res += lock.write(b"0").unwrap();
                            }
                            res += lock.write(&minute_arr).unwrap();
                        };
                    }
                    Some(res)
                }
                Name::MinorDays(day) => lock
                    .write(print::minor_holidays(day, args.language).as_bytes())
                    .ok(),
                Name::CustomHoliday(custom_holiday) => {
                    lock.write(custom_holiday.printable.as_bytes()).ok()
                }
                Name::DailyStudy(daily_study) => match daily_study {
                    DailyStudyOutput::Daf(d) => d.pretty_print(&mut lock, args.language),
                    DailyStudyOutput::RambamThreeChapters(three_chapter) => {
                        three_chapter.pretty_print(&mut lock, args.language)
                    }
                    DailyStudyOutput::RambamOneChapters(one_chapter) => {
                        one_chapter.pretty_print(&mut lock, args.language)
                    }
                    DailyStudyOutput::YerushalmiYomi(yerushalmi_yomi) => {
                        yerushalmi_yomi.pretty_print(&mut lock, args.language)
                    }
                },
                Name::IsraeliHoliday(israeli_holidays) => {
                    israeli_holidays.pretty_print(&mut lock, args.language)
                }
                Name::ChabadHoliday(chabad_holidays) => {
                    chabad_holidays.pretty_print(&mut lock, args.language)
                }
                Name::ShabbosMevarchim(shabbos_mevarchim) => {
                    shabbos_mevarchim.pretty_print(&mut lock, args.language)
                }
            };
            lock.write(b"\n").unwrap();
        });
        Ok(())
    }
    fn json_print(&self) -> Result<(), AppError> {
        println!("{}", serde_json::to_string(&self).unwrap());
        Ok(())
    }
}

impl Return {
    fn print(&self, args: &MainArgs) -> Result<(), AppError> {
        match args.output_type {
            OutputType::JSON => self.json_print(),
            OutputType::Pretty | OutputType::Regular => self.pretty_print(args),
        }
    }
}

type DailyStudyEvents = Vec<DailyStudy>;

trait GetDayVal {
    fn get_day_val(&self, start_year: u64, last_year: u64) -> Vec<DayVal>;
}

impl GetDayVal for DailyStudyEvents {
    fn get_day_val(&self, start_year: u64, last_year: u64) -> Vec<DayVal> {
        use std::num::NonZeroI8;
        if self.is_empty() {
            return vec![];
        }
        let first_day: DateTime<Utc> =
            HebrewDate::from_ymd(start_year, HebrewMonth::Tishrei, NonZeroI8::new(1).unwrap())
                .unwrap()
                .try_into()
                .unwrap();
        let last_day: DateTime<Utc> =
            HebrewDate::from_ymd(last_year, HebrewMonth::Elul, NonZeroI8::new(29).unwrap())
                .unwrap()
                .try_into()
                .unwrap();
        let mut return_val = Vec::new();
        let mut i = first_day;
        while i <= last_day {
            for event in self.iter() {
                match event {
                    DailyStudy::DafYomi => {
                        let first_day_of_second_cycle = Utc.ymd(1975, 6, 23).and_hms(18, 0, 0);
                        if i >= first_day_of_second_cycle {
                            let diff = i - first_day_of_second_cycle;
                            let d = DayVal {
                                day: i,
                                name: Name::DailyStudy(DailyStudyOutput::Daf(Daf::from_days(
                                    (diff.num_days() % 2711).try_into().unwrap(),
                                    &GEMARAS_SECOND_CYCLE,
                                ))),
                                candle_lighting: None,
                            };
                            return_val.push(d);
                        } else {
                            let first_day_of_first_cycle = Utc.ymd(1923, 9, 10).and_hms(18, 0, 0);
                            if i >= first_day_of_first_cycle {
                                let diff = i - first_day_of_first_cycle;
                                let d = DayVal {
                                    day: i,
                                    candle_lighting: None,
                                    name: Name::DailyStudy(DailyStudyOutput::Daf(Daf::from_days(
                                        (diff.num_days() % 2702).try_into().unwrap(),
                                        &GEMARAS_FIRST_CYCLE,
                                    ))),
                                };
                                return_val.push(d);
                            }
                        }
                    }
                    DailyStudy::Rambam(chapters) => {
                        let first_day = Utc.ymd(1984, 4, 27).and_hms(18, 0, 0);
                        let diff: Duration = i - first_day;
                        if i >= first_day {
                            match chapters {
                                RambamChapters::One => {
                                    let d = DayVal {
                                        candle_lighting: None,
                                        day: i,
                                        name: Name::DailyStudy(
                                            DailyStudyOutput::RambamOneChapters(
                                                RambamChapter::from_days(
                                                    (diff.num_days() % 1017).try_into().unwrap(),
                                                ),
                                            ),
                                        ),
                                    };
                                    return_val.push(d);
                                }
                                RambamChapters::Three => {
                                    let d = DayVal {
                                        candle_lighting: None,
                                        day: i,
                                        name: Name::DailyStudy(
                                            DailyStudyOutput::RambamThreeChapters(
                                                RambamThreeChapter::from_days(
                                                    (diff.num_days() % (1017 / 3))
                                                        .try_into()
                                                        .unwrap(),
                                                ),
                                            ),
                                        ),
                                    };
                                    return_val.push(d);
                                }
                            }
                        }
                    }
                    DailyStudy::YerushalmiYomi => {
                        let first_day_of_yerushalmi_yomi = Utc.ymd(1980, 2, 1).and_hms(18, 0, 0);
                        if i >= first_day_of_yerushalmi_yomi {
                            let cur_hebrew_day: HebrewDate = i.try_into().unwrap();
                            let first_hebrew_day_of_yerushalmi_yomi: HebrewDate =
                                first_day_of_yerushalmi_yomi.try_into().unwrap();
                            let amnt_years =
                                cur_hebrew_day.year() - first_hebrew_day_of_yerushalmi_yomi.year();

                            let diff_days = i - first_day_of_yerushalmi_yomi;

                            let this_years_tisha_beav = HebrewYear::new(cur_hebrew_day.year())
                                .unwrap()
                                .get_holidays(Location::Chul, &[TorahReadingType::Chol])
                                .into_iter()
                                .find(|x| x.name() == TorahReading::Chol(NineAv))
                                .unwrap()
                                .day();
                            if !(cur_hebrew_day.month() == HebrewMonth::Tishrei
                                && cur_hebrew_day.day() == NonZeroI8::new(10).unwrap())
                                && !(cur_hebrew_day == this_years_tisha_beav)
                            {
                                let amnt_yom_kippur_this_year = if cur_hebrew_day.month()
                                    == HebrewMonth::Tishrei
                                    && cur_hebrew_day.day() < NonZeroI8::new(10).unwrap()
                                {
                                    0
                                } else {
                                    1
                                };
                                let amnt_tisha_beav_this_year =
                                    if cur_hebrew_day < this_years_tisha_beav {
                                        0
                                    } else {
                                        1
                                    };
                                let amnt_yom_kippur = if amnt_years == 0 {
                                    0
                                } else if amnt_years == 1 {
                                    amnt_yom_kippur_this_year
                                } else {
                                    amnt_years - 1 + amnt_yom_kippur_this_year
                                };
                                let amnt_tisha_beav = if amnt_years == 0 {
                                    amnt_tisha_beav_this_year
                                } else if amnt_years == 1 {
                                    amnt_tisha_beav_this_year + 1
                                } else {
                                    amnt_years + amnt_tisha_beav_this_year
                                };
                                if diff_days.num_days() > 0 {
                                    let d = DayVal {
                                        day: i,
                                        name: Name::DailyStudy(DailyStudyOutput::YerushalmiYomi(
                                            YerushalmiYomi::from_days(
                                                ((diff_days.num_days() as u64
                                                    - amnt_tisha_beav
                                                    - amnt_yom_kippur)
                                                    % (1563 - 5 - 4))
                                                    .try_into()
                                                    .unwrap(),
                                            ),
                                        )),
                                        candle_lighting: None,
                                    };
                                    return_val.push(d);
                                }
                            }
                        }
                    }
                };
            }
            if i.weekday() == Weekday::Sun {}
            i = i + Duration::days(1);
        }
        return_val
    }
}

impl Runnable for ListArgs {
    fn run(&self, args: &MainArgs) -> Result<(), AppError> {
        let main_events = self
            .events
            .iter()
            .filter_map(|x| {
                if let Event::TorahReadingType(trr) = x {
                    Some(*trr)
                } else {
                    None
                }
            })
            .collect::<Vec<TorahReadingType>>();

        let custom_events = self
            .events
            .iter()
            .filter_map(|x| {
                if let Event::CustomHoliday(custom_holiday) = x {
                    Some(custom_holiday.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<CustomHoliday>>();
        let daily_study_events = self
            .events
            .iter()
            .filter_map(|x| {
                if let Event::DailyStudy(daily_study) = x {
                    Some(daily_study.clone())
                } else {
                    None
                }
            })
            .collect::<DailyStudyEvents>();
        let result: Result<Return, AppError> = match self.year {
            YearType::Hebrew(year) => {
                HebrewYear::new(year)?;
                HebrewYear::new(year + self.amnt_years)?;
                let mut part1 = get_list(
                    year,
                    year + self.amnt_years,
                    self.location,
                    &self.events,
                    &main_events,
                    &custom_events,
                    self.exact_days,
                    &self.city,
                )?;
                part1.extend(daily_study_events.get_day_val(year, year + self.amnt_years - 1));
                Ok(Return { list: part1 })
            }

            YearType::Gregorian(year) => {
                let orig_jan_1 = Utc.ymd(year as i32 - 1, 12, 31).and_hms(18, 0, 0);
                let last_jan_1 = Utc
                    .ymd((year + self.amnt_years + 1) as i32, 1, 1)
                    .and_hms(18, 0, 0);
                let that_year = HebrewDate::try_from(orig_jan_1).unwrap().year();
                let last_year = HebrewDate::try_from(last_jan_1).unwrap().year();
                let mut part1 = get_list(
                    that_year,
                    last_year,
                    self.location,
                    &self.events,
                    &main_events,
                    &custom_events,
                    self.exact_days,
                    &self.city,
                )?;
                part1.extend(daily_study_events.get_day_val(that_year, last_year));
                let mut part2: Vec<DayVal> = Vec::with_capacity((self.amnt_years as usize) * 100);
                part1
                    .into_iter()
                    .filter(|x| x.day > Utc.ymd(year as i32, 1, 1).and_hms(0, 0, 0))
                    .filter(|x| {
                        x.day
                            < Utc
                                .ymd((year + self.amnt_years) as i32, 1, 1)
                                .and_hms(0, 0, 0)
                    })
                    .for_each(|x| part2.push(x));

                Ok(Return { list: part2 })
            }
        };
        let mut result1 = result?;
        if !self.no_sort {
            result1.list.par_sort_unstable_by(|a, b| a.day.cmp(&b.day));
        }
        result1.print(args)?;
        Ok(())
    }
}

fn get_list(
    year: u64,
    last_year: u64,
    location: Location,
    events: &[Event],
    main_events: &Vec<TorahReadingType>,
    custom_events: &Vec<CustomHoliday>,
    exact_days: bool,
    city: &Option<City>,
) -> Result<Vec<DayVal>, AppError> {
    let amnt_years = last_year - year;
    let mut part1: Vec<Vec<DayVal>> = Vec::with_capacity(amnt_years as usize);
    HebrewYear::new(year)?;
    HebrewYear::new(year + amnt_years)?;

    (0 as u32..(amnt_years as u32))
        .into_par_iter()
        .map(|x| {
            let mut ret: Vec<DayVal> = Vec::with_capacity(200);
            let year = HebrewYear::new(x as u64 + year).unwrap();

            ret.extend(
                year.get_holidays(location, &main_events)
                    .into_iter()
                    .map(|x| {
                        let day: DateTime<Utc> = x.day().into();
                        let mut light_on_time = false;
                        let mut is_shabbos = false;
                        if let TorahReading::Shabbos(_) = x.name() {
                            light_on_time = true;
                            is_shabbos = true;
                        }
                        if day.weekday() == Weekday::Fri {
                            light_on_time = true;
                            is_shabbos = true;
                        }
                        let mut is_yom_tov = false;
                        if let TorahReading::YomTov(yt) = x.name() {
                            match yt {
                                heca_lib::prelude::YomTov::RoshHashanah2 => {
                                    is_yom_tov = true;
                                }

                                heca_lib::prelude::YomTov::RoshHashanah1
                                | heca_lib::prelude::YomTov::YomKippur
                                | heca_lib::prelude::YomTov::Sukkos1
                                | heca_lib::prelude::YomTov::ShminiAtzeres
                                | heca_lib::prelude::YomTov::Pesach1
                                | heca_lib::prelude::YomTov::Pesach7
                                | heca_lib::prelude::YomTov::Shavuos1 => {
                                    is_yom_tov = true;
                                    if day.weekday() == Weekday::Sat {
                                        light_on_time = false;
                                    } else {
                                        light_on_time = true;
                                    }
                                }

                                yt => match location {
                                    Location::Chul => {
                                        if yt == heca_lib::prelude::YomTov::Sukkos2
                                            || yt == heca_lib::prelude::YomTov::SimchasTorah
                                            || yt == heca_lib::prelude::YomTov::Pesach2
                                            || yt == heca_lib::prelude::YomTov::Pesach8
                                            || yt == heca_lib::prelude::YomTov::Shavuos2
                                        {
                                            is_yom_tov = true;
                                        } else {
                                            is_yom_tov = false;
                                        }
                                    }
                                    Location::Israel => {
                                        is_yom_tov = false;
                                    }
                                },
                            };
                        };
                        if is_shabbos || is_yom_tov {
                            let candle_lighting = if let Some(city) = city {
                                use zmanim::prelude::Zmanim;
                                let date: NaiveDate = day.date().naive_local();
                                if light_on_time {
                                    if let Some(time) = zmanim::get(
                                        &Zmanim::Sunset,
                                        city.latitude,
                                        city.longitude,
                                        date,
                                        &city.time_zone,
                                    ) {
                                        Some(
                                            time - Duration::minutes(
                                                city.candlelighting_to_sunset as i64 - 1,
                                            ),
                                        )
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            DayVal {
                                day,
                                name: Name::TorahReading(x.name()),
                                candle_lighting: Some(candle_lighting),
                            }
                        } else {
                            DayVal {
                                day,
                                name: Name::TorahReading(x.name()),
                                candle_lighting: None,
                            }
                        }
                    }),
            );

            if events.contains(&Event::MinorHoliday(MinorHoliday::Omer)) {
                ret.extend_from_slice(&get_omer(&year));
            }
            if events.contains(&Event::IsraeliHolidays) {
                ret.extend_from_slice(&israeli_holidays::get(&year, exact_days));
            }
            if events.contains(&Event::ChabadHolidays) {
                ret.extend_from_slice(&chabad_holidays::get(&year));
            }
            if events.contains(&Event::ShabbosMevarchim) {
                ret.extend_from_slice(&shabbos_mevarchim::get(&year));
            }
            if events.contains(&Event::MinorHoliday(MinorHoliday::Minor)) {
                ret.extend(get_minor_holidays(&year));
            }
            custom_events.iter().for_each(|x| {
                if let Ok(day) = year.get_hebrew_date(x.date.month, x.date.day) {
                    let d = DayVal {
                        name: Name::CustomHoliday(x.clone()),
                        day: day.try_into().unwrap(),
                        candle_lighting: None,
                    };
                    ret.push(d);
                } else if let Some(not_exists) = &x.if_not_exists {
                    not_exists.iter().for_each(|day_month| {
                        if let Ok(day) = year.get_hebrew_date(day_month.month, day_month.day) {
                            let d = DayVal {
                                name: Name::CustomHoliday(x.clone()),
                                day: day.into(),
                                candle_lighting: None,
                            };
                            ret.push(d);
                        }
                    });
                }
            });

            ret
        })
        .collect_into_vec(&mut part1);
    let mut part2: Vec<DayVal> = Vec::with_capacity(amnt_years as usize);
    part1.into_iter().flatten().for_each(|a| {
        part2.push(a);
    });
    Ok(part2)
}

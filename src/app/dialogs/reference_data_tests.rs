use super::*;

#[test]
fn a_mode_is_greyed_until_its_download_is_switched_on() {
    let mut r = octa::reference::ReferenceSettings::default();
    assert!(mode_why(Mode::Currency, &r).is_some());
    assert!(mode_why(Mode::Places, &r).is_some());
    r.exchange_rates.enabled = true;
    assert!(mode_why(Mode::Currency, &r).is_none());
    r.de_postcode_municipality.enabled = true;
    assert!(mode_why(Mode::Places, &r).is_none());
}

#[test]
fn rounding_follows_the_mode_and_the_typed_decimals() {
    let mut st = ReferenceState::default();
    assert_eq!(st.rounding(), Ok(Rounding::Usual));
    st.round = RoundChoice::To;
    st.decimals_buf = "3".into();
    assert_eq!(st.rounding(), Ok(Rounding::Decimals(3)));
    st.decimals_buf = "-1".into();
    assert!(st.rounding().is_err());
    st.set_mode(Mode::Inflation);
    st.round = RoundChoice::Usual;
    assert_eq!(st.rounding(), Ok(Rounding::Decimals(2)));
}

#[test]
fn the_inflation_date_is_a_column_or_one_typed_month() {
    let mut st = ReferenceState::default();
    assert_eq!(st.inflation_date(), None);
    st.date = Some(2);
    assert_eq!(st.inflation_date(), Some(Pick::Column(2)));
    st.date_col = false;
    st.date_year = 2015;
    st.date_month = 3;
    assert_eq!(st.inflation_date(), Some(Pick::Fixed("2015-03".into())));
}

#[test]
fn kreis_is_left_out_until_german_codes_are_stored() {
    let mut st = ReferenceState::default();
    assert!(!st.offers(Output::Kreis) && !st.offers(Output::KreisName));
    assert!(st.offers(Output::Municipality));
    st.kreis_stored = true;
    assert!(st.offers(Output::Kreis));
}
